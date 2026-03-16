//! # Risk Engine
//!
//! Core risk management component for pre-trade validation and portfolio monitoring.
//!
//! This module provides comprehensive risk checks including:
//! - Position size limits (per symbol)
//! - Daily loss limits
//! - Drawdown limits (max % from peak)
//! - Symbol exposure limits
//! - Total exposure limit
//! - Circuit breaker pattern
//! - Kill switch functionality
//!
//! ## Architecture
//!
//! The RiskEngine uses a thread-safe design with RwLock for shared state:
//! - Read-heavy operations (pre-trade checks) use read locks
//! - State modifications use write locks
//!
//! ## Usage
//!
//! ```rust
//! use application::risk::{RiskEngine, RiskConfig};
//! use domain::entities::{Order, Position};
//!
//! fn validate_order(engine: &RiskEngine, order: &Order, positions: &[Position]) {
//!     match engine.check_pre_trade(order, positions).unwrap() {
//!         RiskDecision::Allow => println!("Order approved"),
//!         RiskDecision::Reject { reason } => println!("Order rejected: {}", reason),
//!         RiskDecision::Reduce { max_allowed } => println!("Reduce to: {}", max_allowed),
//!     }
//! }
//! ```

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use chrono::{DateTime, Utc, NaiveDate};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tracing::{info, warn, error, instrument};

use domain::{
    entities::{Order, Position},
    values::{Symbol, Quantity, Money, Currency, Price},
    errors::DomainError,
};

// =============================================================================
// RISK DECISION
// =============================================================================

/// Decision outcome from risk engine validation.
///
/// Represents the result of a pre-trade risk check.
#[derive(Debug, Clone, PartialEq)]
pub enum RiskDecision {
    /// Order approved for execution.
    Allow,
    /// Order rejected with specific reason.
    Reject { reason: String },
    /// Reduce quantity to maximum allowed.
    Reduce { max_allowed: Quantity },
}

impl RiskDecision {
    /// Returns true if the order is approved.
    ///
    /// # Examples
    ///
    /// ```
    /// use application::risk::RiskDecision;
    ///
    /// let decision = RiskDecision::Allow;
    /// assert!(decision.is_allowed());
    /// ```
    #[must_use]
    pub fn is_allowed(&self) -> bool {
        matches!(self, RiskDecision::Allow)
    }

    /// Returns true if the order is rejected.
    ///
    /// # Examples
    ///
    /// ```
    /// use application::risk::RiskDecision;
    ///
    /// let decision = RiskDecision::Reject { reason: "test".to_string() };
    /// assert!(decision.is_rejected());
    /// ```
    #[must_use]
    pub fn is_rejected(&self) -> bool {
        matches!(self, RiskDecision::Reject { .. })
    }

    /// Returns true if the order should be reduced.
    #[must_use]
    pub fn is_reduce(&self) -> bool {
        matches!(self, RiskDecision::Reduce { .. })
    }
}

// =============================================================================
// RISK CONFIGURATION
// =============================================================================

/// Configuration for risk engine limits and thresholds.
///
/// All monetary values use [`Money`] for type safety.
/// All percentage values use [`Decimal`] (e.g., 0.05 for 5%).
#[derive(Debug, Clone)]
pub struct RiskConfig {
    /// Maximum position size per symbol.
    pub max_position_size: HashMap<Symbol, Quantity>,
    /// Maximum total exposure (sum of absolute position values).
    pub max_total_exposure: Money,
    /// Daily loss limit (stored as positive value, compared against absolute loss).
    pub daily_loss_limit: Money,
    /// Maximum drawdown percentage from peak (e.g., 0.05 = 5%).
    pub max_drawdown_pct: Decimal,
    /// Maximum exposure per symbol (in monetary value).
    pub max_symbol_exposure: HashMap<Symbol, Money>,
    /// Whether kill switch is enabled.
    pub kill_switch_enabled: bool,
    /// Number of consecutive failures to trigger circuit breaker.
    pub circuit_breaker_failures: u32,
    /// Circuit breaker timeout in seconds before attempting recovery.
    pub circuit_breaker_timeout_secs: u64,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            max_position_size: HashMap::new(),
            max_total_exposure: Money::new(dec!(1_000_000), Currency::USD)
                .expect("Valid money"), // $1M
            daily_loss_limit: Money::new(dec!(50_000), Currency::USD)
                .expect("Valid money"), // $50K (stored as positive)
            max_drawdown_pct: dec!(0.10), // 10%
            max_symbol_exposure: HashMap::new(),
            kill_switch_enabled: true,
            circuit_breaker_failures: 5,
            circuit_breaker_timeout_secs: 300, // 5 min
        }
    }
}

impl RiskConfig {
    /// Creates a new configuration with default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the maximum position size for a symbol.
    pub fn with_position_limit(mut self, symbol: Symbol, max_qty: Quantity) -> Self {
        self.max_position_size.insert(symbol, max_qty);
        self
    }

    /// Sets the maximum symbol exposure.
    pub fn with_symbol_exposure(mut self, symbol: Symbol, max_exp: Money) -> Self {
        self.max_symbol_exposure.insert(symbol, max_exp);
        self
    }

    /// Sets the total exposure limit.
    pub fn with_total_exposure(mut self, max_exp: Money) -> Self {
        self.max_total_exposure = max_exp;
        self
    }

    /// Sets the daily loss limit.
    pub fn with_daily_loss_limit(mut self, limit: Money) -> Self {
        self.daily_loss_limit = limit;
        self
    }

    /// Sets the maximum drawdown percentage.
    pub fn with_max_drawdown(mut self, pct: Decimal) -> Self {
        self.max_drawdown_pct = pct;
        self
    }
}

// =============================================================================
// CIRCUIT BREAKER STATE
// =============================================================================

/// State of the circuit breaker.
///
/// Follows the standard circuit breaker pattern:
/// - Closed: Normal operation
/// - Open: Trading blocked due to failures
/// - HalfOpen: Testing if service has recovered
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CircuitBreakerState {
    /// Normal operation - all trading allowed.
    Closed,
    /// Trading blocked - too many failures.
    Open,
    /// Testing recovery - limited trading allowed.
    HalfOpen,
}

impl std::fmt::Display for CircuitBreakerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitBreakerState::Closed => write!(f, "CLOSED"),
            CircuitBreakerState::Open => write!(f, "OPEN"),
            CircuitBreakerState::HalfOpen => write!(f, "HALF_OPEN"),
        }
    }
}

impl CircuitBreakerState {
    /// Returns true if trading is allowed in this state.
    #[must_use]
    pub fn is_trading_allowed(&self) -> bool {
        matches!(self, CircuitBreakerState::Closed | CircuitBreakerState::HalfOpen)
    }
}

// =============================================================================
// RISK STATE
// =============================================================================

/// Current state of the risk engine.
///
/// Tracks portfolio metrics, limits, and circuit breaker status.
/// This state is thread-safe and persisted in the RiskEngine.
#[derive(Debug, Clone)]
pub struct RiskState {
    /// Current date for daily reset tracking.
    pub current_date: NaiveDate,
    /// Daily realized P&L.
    pub daily_realized_pnl: Money,
    /// Daily unrealized P&L.
    pub daily_unrealized_pnl: Money,
    /// Peak equity value (for drawdown calculation).
    pub equity_peak: Money,
    /// Current equity value.
    pub current_equity: Money,
    /// Whether kill switch is currently active.
    pub kill_switch_triggered: bool,
    /// Current circuit breaker state.
    pub circuit_breaker: CircuitBreakerState,
    /// Consecutive failure counter.
    pub consecutive_failures: u32,
    /// When the circuit breaker was opened.
    pub circuit_breaker_opened_at: Option<DateTime<Utc>>,
    /// History of recent decisions (last 100).
    pub decision_history: Vec<(DateTime<Utc>, RiskDecision)>,
}

impl Default for RiskState {
    fn default() -> Self {
        let initial_equity = Money::new(dec!(100_000), Currency::USD)
            .expect("Valid initial equity");
        Self {
            current_date: Utc::now().date_naive(),
            daily_realized_pnl: Money::new(dec!(0), Currency::USD)
                .expect("Valid money"),
            daily_unrealized_pnl: Money::new(dec!(0), Currency::USD)
                .expect("Valid money"),
            equity_peak: initial_equity,
            current_equity: initial_equity,
            kill_switch_triggered: false,
            circuit_breaker: CircuitBreakerState::Closed,
            consecutive_failures: 0,
            circuit_breaker_opened_at: None,
            decision_history: Vec::with_capacity(100),
        }
    }
}

impl RiskState {
    /// Returns the total daily P&L (realized + unrealized).
    #[must_use]
    pub fn total_daily_pnl(&self) -> Decimal {
        self.daily_realized_pnl.amount() + self.daily_unrealized_pnl.amount()
    }

    /// Calculates current drawdown as a percentage.
    #[must_use]
    pub fn current_drawdown_pct(&self) -> Decimal {
        if self.equity_peak.amount() > dec!(0) {
            (self.equity_peak.amount() - self.current_equity.amount())
                / self.equity_peak.amount()
        } else {
            dec!(0)
        }
    }

    /// Returns true if kill switch is active.
    #[must_use]
    pub fn is_kill_switch_active(&self) -> bool {
        self.kill_switch_triggered
    }
}

// =============================================================================
// RISK ENGINE
// =============================================================================

/// Core risk engine for pre-trade validation and monitoring.
///
/// The RiskEngine validates orders before execution and monitors
/// portfolio risk metrics during trading. It implements:
///
/// - Position size limits
/// - Daily loss limits
/// - Drawdown limits
/// - Exposure limits
/// - Circuit breaker pattern
/// - Kill switch functionality
///
/// ## Thread Safety
///
/// The engine uses `Arc<RwLock<RiskState>>` for thread-safe state management:
/// - Read locks for validation checks
/// - Write locks for state updates
///
/// ## Example
///
/// ```rust
/// use application::risk::{RiskEngine, RiskConfig};
///
/// let config = RiskConfig::default();
/// let engine = RiskEngine::new(config);
/// ```
#[derive(Debug)]
pub struct RiskEngine {
    config: RiskConfig,
    state: Arc<RwLock<RiskState>>,
}

impl RiskEngine {
    /// Creates a new RiskEngine with the given configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use application::risk::{RiskEngine, RiskConfig};
    ///
    /// let config = RiskConfig::default();
    /// let engine = RiskEngine::new(config);
    /// ```
    #[must_use]
    pub fn new(config: RiskConfig) -> Self {
        info!("Initializing RiskEngine with config: {:?}", config);
        Self {
            config,
            state: Arc::new(RwLock::new(RiskState::default())),
        }
    }

    /// Performs pre-trade validation on an order.
    ///
    /// Checks in order of priority:
    /// 1. Kill switch status
    /// 2. Circuit breaker state
    /// 3. Position size limits
    /// 4. Daily loss limits
    /// 5. Drawdown limits
    /// 6. Total exposure limits
    /// 7. Symbol exposure limits
    ///
    /// # Arguments
    ///
    /// * `order` - The order to validate
    /// * `positions` - Current open positions
    /// * `current_price` - Current market price for exposure calculation
    ///
    /// # Returns
    ///
    /// Returns [`RiskDecision`] indicating whether the order is allowed.
    #[instrument(skip(self, order, positions, current_price))]
    pub fn check_pre_trade(
        &self,
        order: &Order,
        positions: &[Position],
        current_price: Price,
    ) -> Result<RiskDecision, DomainError> {
        let timestamp = Utc::now();

        // 1. Check kill switch
        let decision = {
            let state = self.state.read().map_err(|_| {
                DomainError::unknown("Risk state lock poisoned")
            })?;

            if state.kill_switch_triggered {
                Some(RiskDecision::Reject {
                    reason: "KILL SWITCH ACTIVE - Trading disabled".to_string(),
                })
            } else if state.circuit_breaker == CircuitBreakerState::Open {
                Some(RiskDecision::Reject {
                    reason: "CIRCUIT BREAKER OPEN - Trading suspended".to_string(),
                })
            } else {
                None
            }
        };

        if let Some(d) = decision {
            self.record_decision(timestamp, d.clone())?;
            return Ok(d);
        }

        // 2. Position size limit check
        if let Some(decision) = self.check_position_size(order, positions)? {
            self.record_decision(timestamp, decision.clone())?;
            return Ok(decision);
        }

        // 3. Daily loss limit check
        if let Some(decision) = self.check_daily_loss_limit()? {
            self.record_decision(timestamp, decision.clone())?;
            return Ok(decision);
        }

        // 4. Drawdown check
        if let Some(decision) = self.check_drawdown_limit()? {
            self.record_decision(timestamp, decision.clone())?;
            return Ok(decision);
        }

        // 5. Total exposure check
        if let Some(decision) = self.check_total_exposure(order, positions, current_price)? {
            self.record_decision(timestamp, decision.clone())?;
            return Ok(decision);
        }

        // 6. Symbol exposure check
        if let Some(decision) = self.check_symbol_exposure(order, positions, current_price)? {
            self.record_decision(timestamp, decision.clone())?;
            return Ok(decision);
        }

        // All checks passed
        let decision = RiskDecision::Allow;
        self.record_decision(timestamp, decision.clone())?;
        info!("Pre-trade check passed for order: {:?}", order.id());
        Ok(decision)
    }

    /// Records a decision in the history.
    fn record_decision(&self, timestamp: DateTime<Utc>, decision: RiskDecision) -> Result<(), DomainError> {
        let mut state = self.state.write().map_err(|_| {
            DomainError::unknown("Risk state lock poisoned")
        })?;

        // Keep only last 100 decisions
        if state.decision_history.len() >= 100 {
            state.decision_history.remove(0);
        }
        state.decision_history.push((timestamp, decision));
        Ok(())
    }

    /// Checks position size limits.
    fn check_position_size(
        &self,
        order: &Order,
        positions: &[Position],
    ) -> Result<Option<RiskDecision>, DomainError> {
        let Some(max_size) = self.config.max_position_size.get(order.symbol()) else {
            return Ok(None);
        };

        // Calculate current position for this symbol
        let current_qty: Decimal = positions
            .iter()
            .filter(|p| p.symbol() == order.symbol())
            .map(|p| p.quantity().inner())
            .sum();

        let order_qty = order.remaining_quantity().inner();
        let new_total = current_qty + order_qty;

        if new_total > max_size.inner() {
            let remaining = max_size.inner() - current_qty;

            if remaining <= dec!(0) {
                warn!(
                    "Position size limit exceeded for {}: max {}, current {}",
                    order.symbol(), max_size.inner(), current_qty
                );
                return Ok(Some(RiskDecision::Reject {
                    reason: format!(
                        "Position size limit exceeded for {}: max {}, current {}",
                        order.symbol(), max_size.inner(), current_qty
                    ),
                }));
            } else {
                warn!(
                    "Position size limit would be exceeded for {}: reducing from {} to {}",
                    order.symbol(), order_qty, remaining
                );
                let max_allowed = Quantity::new(remaining)
                    .map_err(|e| DomainError::validation(format!("Invalid quantity: {}", e)))?;
                return Ok(Some(RiskDecision::Reduce { max_allowed }));
            }
        }

        Ok(None)
    }

    /// Checks daily loss limit.
    fn check_daily_loss_limit(&self) -> Result<Option<RiskDecision>, DomainError> {
        let state = self.state.read().map_err(|_| {
            DomainError::unknown("Risk state lock poisoned")
        })?;

        let total_pnl = state.total_daily_pnl();

        // daily_loss_limit is stored as positive, compare absolute loss
        if total_pnl < -self.config.daily_loss_limit.amount() {
            warn!(
                "Daily loss limit reached: {} < -{}",
                total_pnl, self.config.daily_loss_limit.amount()
            );
            return Ok(Some(RiskDecision::Reject {
                reason: format!(
                    "Daily loss limit reached: {} < -{}",
                    total_pnl, self.config.daily_loss_limit.amount()
                ),
            }));
        }

        Ok(None)
    }

    /// Checks drawdown limit.
    fn check_drawdown_limit(&self) -> Result<Option<RiskDecision>, DomainError> {
        let state = self.state.read().map_err(|_| {
            DomainError::unknown("Risk state lock poisoned")
        })?;

        let drawdown_pct = state.current_drawdown_pct();

        if drawdown_pct > self.config.max_drawdown_pct {
            error!(
                "Max drawdown exceeded: {:.2}% > {:.2}%",
                drawdown_pct * dec!(100),
                self.config.max_drawdown_pct * dec!(100)
            );
            return Ok(Some(RiskDecision::Reject {
                reason: format!(
                    "Max drawdown exceeded: {:.2}% > {:.2}%",
                    drawdown_pct * dec!(100),
                    self.config.max_drawdown_pct * dec!(100)
                ),
            }));
        }

        // Warning if approaching limit (within 20% of limit)
        let warning_threshold = self.config.max_drawdown_pct * dec!(0.8);
        if drawdown_pct > warning_threshold {
            warn!(
                "Drawdown approaching limit: {:.2}% (limit: {:.2}%)",
                drawdown_pct * dec!(100),
                self.config.max_drawdown_pct * dec!(100)
            );
        }

        Ok(None)
    }

    /// Checks total exposure limit.
    fn check_total_exposure(
        &self,
        order: &Order,
        positions: &[Position],
        current_price: Price,
    ) -> Result<Option<RiskDecision>, DomainError> {
        let total_exposure = self.calculate_total_exposure(positions)?;

        // Calculate order notional value
        let order_value = current_price.inner() * order.remaining_quantity().inner();
        let new_exposure = total_exposure.amount() + order_value.abs();

        if new_exposure > self.config.max_total_exposure.amount() {
            warn!(
                "Total exposure limit exceeded: {} > {}",
                new_exposure, self.config.max_total_exposure.amount()
            );
            return Ok(Some(RiskDecision::Reject {
                reason: format!(
                    "Total exposure limit exceeded: {} > {}",
                    new_exposure, self.config.max_total_exposure.amount()
                ),
            }));
        }

        Ok(None)
    }

    /// Checks symbol exposure limit.
    fn check_symbol_exposure(
        &self,
        order: &Order,
        positions: &[Position],
        current_price: Price,
    ) -> Result<Option<RiskDecision>, DomainError> {
        let Some(max_symbol_exp) = self.config.max_symbol_exposure.get(order.symbol()) else {
            return Ok(None);
        };

        let symbol_exposure = self.calculate_symbol_exposure(order.symbol(), positions)?;

        // Calculate order value
        let order_value = current_price.inner() * order.remaining_quantity().inner();
        let new_symbol_exposure = symbol_exposure.amount() + order_value.abs();

        if new_symbol_exposure > max_symbol_exp.amount() {
            warn!(
                "Symbol exposure limit exceeded for {}: {} > {}",
                order.symbol(), new_symbol_exposure, max_symbol_exp.amount()
            );
            return Ok(Some(RiskDecision::Reject {
                reason: format!(
                    "Symbol exposure limit exceeded for {}: {} > {}",
                    order.symbol(), new_symbol_exposure, max_symbol_exp.amount()
                ),
            }));
        }

        Ok(None)
    }

    /// Calculates total portfolio exposure.
    fn calculate_total_exposure(&self, positions: &[Position]) -> Result<Money, DomainError> {
        let total: Decimal = positions
            .iter()
            .map(|p| p.market_value().amount())
            .sum();

        Money::new(total, Currency::USD)
            .map_err(|e| DomainError::validation(format!("Invalid exposure: {}", e)))
    }

    /// Calculates exposure for a specific symbol.
    fn calculate_symbol_exposure(
        &self,
        symbol: &Symbol,
        positions: &[Position],
    ) -> Result<Money, DomainError> {
        let total: Decimal = positions
            .iter()
            .filter(|p| p.symbol() == symbol)
            .map(|p| p.market_value().amount())
            .sum();

        Money::new(total, Currency::USD)
            .map_err(|e| DomainError::validation(format!("Invalid exposure: {}", e)))
    }

    /// Updates risk state after a fill.
    ///
    /// Records realized P&L and resets consecutive failures on success.
    #[instrument(skip(self, realized_pnl))]
    pub fn update_on_fill(
        &self,
        realized_pnl: Option<Money>,
    ) -> Result<(), DomainError> {
        let mut state = self.state.write().map_err(|_| {
            DomainError::unknown("Risk state lock poisoned")
        })?;

        // Update daily realized P&L
        if let Some(pnl) = realized_pnl {
            let new_pnl = state.daily_realized_pnl.amount() + pnl.amount();
            state.daily_realized_pnl = Money::new(new_pnl, Currency::USD)
                .map_err(|e| DomainError::validation(format!("Invalid P&L: {}", e)))?;
            info!("Updated realized P&L: {}", new_pnl);
        }

        // Reset consecutive failures on successful fill
        if state.consecutive_failures > 0 {
            info!("Resetting consecutive failures from {}", state.consecutive_failures);
            state.consecutive_failures = 0;
        }

        Ok(())
    }

    /// Updates equity and checks drawdown.
    ///
    /// Should be called periodically with current portfolio value.
    #[instrument(skip(self))]
    pub fn update_equity(&self, new_equity: Money) -> Result<(), DomainError> {
        let mut state = self.state.write().map_err(|_| {
            DomainError::unknown("Risk state lock poisoned")
        })?;

        let old_equity = state.current_equity.clone();
        state.current_equity = new_equity.clone();

        // Update peak if new high
        if new_equity.amount() > state.equity_peak.amount() {
            state.equity_peak = new_equity.clone();
            info!("New equity peak: {}", new_equity.amount());
        }

        // Calculate drawdown
        let drawdown_pct = state.current_drawdown_pct();

        if drawdown_pct > dec!(0) {
            info!(
                "Equity update: {} -> {} (drawdown: {:.2}%)",
                old_equity.amount(),
                new_equity.amount(),
                drawdown_pct * dec!(100)
            );

            // Warning if approaching limit
            let warning_threshold = self.config.max_drawdown_pct * dec!(0.8);
            if drawdown_pct > warning_threshold && drawdown_pct <= self.config.max_drawdown_pct {
                warn!(
                    "Drawdown approaching limit: {:.2}% (limit: {:.2}%)",
                    drawdown_pct * dec!(100),
                    self.config.max_drawdown_pct * dec!(100)
                );
            }
        }

        Ok(())
    }

    /// Triggers the kill switch.
    ///
    /// Immediately disables all trading. Requires manual reset.
    #[instrument(skip(self))]
    pub fn trigger_kill_switch(&self, reason: &str) -> Result<(), DomainError> {
        if !self.config.kill_switch_enabled {
            warn!("Kill switch triggered but disabled in config: {}", reason);
            return Ok(());
        }

        let mut state = self.state.write().map_err(|_| {
            DomainError::unknown("Risk state lock poisoned")
        })?;

        state.kill_switch_triggered = true;
        error!("KILL SWITCH TRIGGERED: {}", reason);

        Ok(())
    }

    /// Resets the kill switch.
    ///
    /// # Security
    ///
    /// This should require administrative authorization.
    #[instrument(skip(self))]
    pub fn reset_kill_switch(&self) -> Result<(), DomainError> {
        let mut state = self.state.write().map_err(|_| {
            DomainError::unknown("Risk state lock poisoned")
        })?;

        state.kill_switch_triggered = false;
        info!("Kill switch reset");

        Ok(())
    }

    /// Returns true if kill switch is active.
    pub fn is_kill_switch_active(&self) -> Result<bool, DomainError> {
        let state = self.state.read().map_err(|_| {
            DomainError::unknown("Risk state lock poisoned")
        })?;

        Ok(state.kill_switch_triggered)
    }

    /// Records a failure and checks circuit breaker.
    ///
    /// Call this when an operation fails (e.g., order rejection, connection error).
    /// Returns the current circuit breaker state.
    #[instrument(skip(self))]
    pub fn record_failure(&self, error: &str) -> Result<CircuitBreakerState, DomainError> {
        let mut state = self.state.write().map_err(|_| {
            DomainError::unknown("Risk state lock poisoned")
        })?;

        state.consecutive_failures += 1;
        warn!(
            "Failure recorded: {} (consecutive: {})",
            error, state.consecutive_failures
        );

        if state.consecutive_failures >= self.config.circuit_breaker_failures {
            if state.circuit_breaker == CircuitBreakerState::Closed {
                state.circuit_breaker = CircuitBreakerState::Open;
                state.circuit_breaker_opened_at = Some(Utc::now());
                error!(
                    "CIRCUIT BREAKER OPENED after {} consecutive failures",
                    state.consecutive_failures
                );
            }
        }

        Ok(state.circuit_breaker)
    }

    /// Checks if circuit breaker can transition to HalfOpen.
    ///
    /// Call this periodically to attempt recovery.
    pub fn check_circuit_breaker_recovery(&self) -> Result<CircuitBreakerState, DomainError> {
        let mut state = self.state.write().map_err(|_| {
            DomainError::unknown("Risk state lock poisoned")
        })?;

        if state.circuit_breaker == CircuitBreakerState::Open {
            if let Some(opened_at) = state.circuit_breaker_opened_at {
                let elapsed = Utc::now().signed_duration_since(opened_at);
                if elapsed.num_seconds() as u64 >= self.config.circuit_breaker_timeout_secs {
                    state.circuit_breaker = CircuitBreakerState::HalfOpen;
                    info!("Circuit breaker entering HALF_OPEN state for recovery test");
                }
            }
        }

        Ok(state.circuit_breaker)
    }

    /// Confirms circuit breaker recovery.
    ///
    /// Call this after a successful operation in HalfOpen state.
    #[instrument(skip(self))]
    pub fn confirm_recovery(&self) -> Result<(), DomainError> {
        let mut state = self.state.write().map_err(|_| {
            DomainError::unknown("Risk state lock poisoned")
        })?;

        if state.circuit_breaker == CircuitBreakerState::HalfOpen {
            state.circuit_breaker = CircuitBreakerState::Closed;
            state.consecutive_failures = 0;
            state.circuit_breaker_opened_at = None;
            info!("Circuit breaker CLOSED - Recovery confirmed");
        }

        Ok(())
    }

    /// Returns the current circuit breaker state.
    pub fn circuit_breaker_state(&self) -> Result<CircuitBreakerState, DomainError> {
        let state = self.state.read().map_err(|_| {
            DomainError::unknown("Risk state lock poisoned")
        })?;

        Ok(state.circuit_breaker)
    }

    /// Returns a copy of the current state.
    pub fn get_state(&self) -> Result<RiskState, DomainError> {
        let state = self.state.read().map_err(|_| {
            DomainError::unknown("Risk state lock poisoned")
        })?;

        Ok(state.clone())
    }

    /// Returns a copy of the configuration.
    pub fn config(&self) -> RiskConfig {
        self.config.clone()
    }

    /// Performs daily reset of risk metrics.
    ///
    /// Call this at the start of each trading day.
    #[instrument(skip(self))]
    pub fn daily_reset(&self) -> Result<(), DomainError> {
        let mut state = self.state.write().map_err(|_| {
            DomainError::unknown("Risk state lock poisoned")
        })?;

        state.current_date = Utc::now().date_naive();
        state.daily_realized_pnl = Money::new(dec!(0), Currency::USD)
            .map_err(|e| DomainError::validation(format!("Invalid P&L: {}", e)))?;
        state.daily_unrealized_pnl = Money::new(dec!(0), Currency::USD)
            .map_err(|e| DomainError::validation(format!("Invalid P&L: {}", e)))?;
        state.consecutive_failures = 0;

        info!("Risk engine daily reset completed for {}", state.current_date);
        Ok(())
    }
}

// =============================================================================
// UNIT TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use domain::entities::{Order, OrderType, Position, PositionDirection};
    use domain::values::{Symbol, Quantity, Price, Currency, Money, Side, TimeInForce};

    fn create_test_order(symbol: &str, qty: Decimal, side: Side) -> Order {
        Order::new(
            uuid::Uuid::new_v4(),
            Symbol::new(symbol).unwrap(),
            side,
            OrderType::Market,
            Quantity::new(qty).unwrap(),
            None,
            None,
        ).unwrap()
    }

    fn create_test_position(symbol: &str, qty: Decimal, side: Side, price: Decimal) -> Position {
        Position::new_with_side(
            Symbol::new(symbol).unwrap(),
            side,
            Quantity::new(qty).unwrap(),
            Price::new(price).unwrap(),
            Currency::USD,
        ).unwrap()
    }

    fn create_test_config() -> RiskConfig {
        RiskConfig {
            max_position_size: HashMap::new(),
            max_total_exposure: Money::new(dec!(1_000_000), Currency::USD).unwrap(),
            daily_loss_limit: Money::new(dec!(50_000), Currency::USD).unwrap(),
            max_drawdown_pct: dec!(0.10),
            max_symbol_exposure: HashMap::new(),
            kill_switch_enabled: true,
            circuit_breaker_failures: 3,
            circuit_breaker_timeout_secs: 60,
        }
    }

    #[test]
    fn test_risk_decision_allow() {
        let decision = RiskDecision::Allow;
        assert!(decision.is_allowed());
        assert!(!decision.is_rejected());
        assert!(!decision.is_reduce());
    }

    #[test]
    fn test_risk_decision_reject() {
        let decision = RiskDecision::Reject {
            reason: "Test rejection".to_string(),
        };
        assert!(!decision.is_allowed());
        assert!(decision.is_rejected());
        assert!(!decision.is_reduce());
    }

    #[test]
    fn test_risk_decision_reduce() {
        let decision = RiskDecision::Reduce {
            max_allowed: Quantity::new(dec!(50)).unwrap(),
        };
        assert!(!decision.is_allowed());
        assert!(!decision.is_rejected());
        assert!(decision.is_reduce());
    }

    #[test]
    fn test_circuit_breaker_state_display() {
        assert_eq!(CircuitBreakerState::Closed.to_string(), "CLOSED");
        assert_eq!(CircuitBreakerState::Open.to_string(), "OPEN");
        assert_eq!(CircuitBreakerState::HalfOpen.to_string(), "HALF_OPEN");
    }

    #[test]
    fn test_circuit_breaker_state_is_trading_allowed() {
        assert!(CircuitBreakerState::Closed.is_trading_allowed());
        assert!(!CircuitBreakerState::Open.is_trading_allowed());
        assert!(CircuitBreakerState::HalfOpen.is_trading_allowed());
    }

    #[test]
    fn test_risk_config_default() {
        let config = RiskConfig::default();
        assert_eq!(config.max_total_exposure.amount(), dec!(1_000_000));
        assert_eq!(config.daily_loss_limit.amount(), dec!(50_000));
        assert_eq!(config.max_drawdown_pct, dec!(0.10));
        assert!(config.kill_switch_enabled);
        assert_eq!(config.circuit_breaker_failures, 5);
        assert_eq!(config.circuit_breaker_timeout_secs, 300);
    }

    #[test]
    fn test_risk_config_builder() {
        let config = RiskConfig::new()
            .with_total_exposure(Money::new(dec!(500_000), Currency::USD).unwrap())
            .with_daily_loss_limit(Money::new(dec!(25_000), Currency::USD).unwrap())
            .with_max_drawdown(dec!(0.05));

        assert_eq!(config.max_total_exposure.amount(), dec!(500_000));
        assert_eq!(config.daily_loss_limit.amount(), dec!(25_000));
        assert_eq!(config.max_drawdown_pct, dec!(0.05));
    }

    #[test]
    fn test_risk_engine_creation() {
        let config = create_test_config();
        let engine = RiskEngine::new(config);

        let state = engine.get_state().unwrap();
        assert!(!state.kill_switch_triggered);
        assert_eq!(state.circuit_breaker, CircuitBreakerState::Closed);
    }

    #[test]
    fn test_position_size_limit_allows_under_limit() {
        let mut config = create_test_config();
        config.max_position_size.insert(Symbol::new("AAPL").unwrap(), Quantity::new(dec!(1000)).unwrap());

        let engine = RiskEngine::new(config);
        let order = create_test_order("AAPL", dec!(100), Side::Buy);
        let positions = vec![];
        let price = Price::new(dec!(150)).unwrap();

        let result = engine.check_pre_trade(&order, &positions, price).unwrap();
        assert!(result.is_allowed());
    }

    #[test]
    fn test_position_size_limit_reduces_when_over_limit() {
        // When order exceeds limit but there's partial space, should Reduce
        let mut config = create_test_config();
        config.max_position_size.insert(Symbol::new("AAPL").unwrap(), Quantity::new(dec!(100)).unwrap());

        let engine = RiskEngine::new(config);
        let order = create_test_order("AAPL", dec!(150), Side::Buy);
        let positions = vec![];
        let price = Price::new(dec!(150)).unwrap();

        let result = engine.check_pre_trade(&order, &positions, price).unwrap();
        assert!(result.is_reduce());
        assert!(matches!(result, RiskDecision::Reduce { max_allowed } if max_allowed.inner() == dec!(100)));
    }

    #[test]
    fn test_position_size_limit_rejects_when_no_space() {
        // When position already at limit, should Reject
        let mut config = create_test_config();
        config.max_position_size.insert(Symbol::new("AAPL").unwrap(), Quantity::new(dec!(100)).unwrap());

        let engine = RiskEngine::new(config);
        let order = create_test_order("AAPL", dec!(10), Side::Buy);
        let positions = vec![create_test_position("AAPL", dec!(100), Side::Buy, dec!(140))];
        let price = Price::new(dec!(150)).unwrap();

        let result = engine.check_pre_trade(&order, &positions, price).unwrap();
        assert!(result.is_rejected());
    }

    #[test]
    fn test_position_size_limit_reduces_when_partial_allowed() {
        let mut config = create_test_config();
        config.max_position_size.insert(Symbol::new("AAPL").unwrap(), Quantity::new(dec!(100)).unwrap());

        let engine = RiskEngine::new(config);
        let order = create_test_order("AAPL", dec!(150), Side::Buy);
        let positions = vec![create_test_position("AAPL", dec!(50), Side::Buy, dec!(140))];
        let price = Price::new(dec!(150)).unwrap();

        let result = engine.check_pre_trade(&order, &positions, price).unwrap();
        assert!(result.is_reduce());
    }

    #[test]
    fn test_kill_switch_blocks_trading() {
        let config = create_test_config();
        let engine = RiskEngine::new(config);
        let order = create_test_order("AAPL", dec!(100), Side::Buy);
        let positions = vec![];
        let price = Price::new(dec!(150)).unwrap();

        // Trigger kill switch
        engine.trigger_kill_switch("Manual test").unwrap();
        assert!(engine.is_kill_switch_active().unwrap());

        // Trading should be blocked
        let result = engine.check_pre_trade(&order, &positions, price).unwrap();
        assert!(result.is_rejected());
        assert!(matches!(result, RiskDecision::Reject { reason } if reason.contains("KILL SWITCH")));
    }

    #[test]
    fn test_kill_switch_reset_allows_trading() {
        let config = create_test_config();
        let engine = RiskEngine::new(config);
        let order = create_test_order("AAPL", dec!(100), Side::Buy);
        let positions = vec![];
        let price = Price::new(dec!(150)).unwrap();

        // Trigger and then reset
        engine.trigger_kill_switch("Test").unwrap();
        engine.reset_kill_switch().unwrap();

        assert!(!engine.is_kill_switch_active().unwrap());

        // Trading should be allowed now
        let result = engine.check_pre_trade(&order, &positions, price).unwrap();
        assert!(result.is_allowed());
    }

    #[test]
    fn test_circuit_breaker_triggers_after_failures() {
        let config = create_test_config();
        let engine = RiskEngine::new(config);

        // Record failures
        for i in 0..3 {
            let state = engine.record_failure(&format!("Failure {}", i)).unwrap();
            if i < 2 {
                assert_eq!(state, CircuitBreakerState::Closed);
            } else {
                assert_eq!(state, CircuitBreakerState::Open);
            }
        }

        assert_eq!(engine.circuit_breaker_state().unwrap(), CircuitBreakerState::Open);
    }

    #[test]
    fn test_circuit_breaker_opens_blocks_trading() {
        let config = create_test_config();
        let engine = RiskEngine::new(config);
        let order = create_test_order("AAPL", dec!(100), Side::Buy);
        let positions = vec![];
        let price = Price::new(dec!(150)).unwrap();

        // Trigger circuit breaker
        for _ in 0..3 {
            engine.record_failure("Test failure").unwrap();
        }

        // Trading should be blocked
        let result = engine.check_pre_trade(&order, &positions, price).unwrap();
        assert!(result.is_rejected());
        assert!(matches!(result, RiskDecision::Reject { reason } if reason.contains("CIRCUIT BREAKER")));
    }

    #[test]
    fn test_circuit_breaker_recovery_flow() {
        let mut config = create_test_config();
        config.circuit_breaker_timeout_secs = 0; // Immediate recovery
        let engine = RiskEngine::new(config);

        // Open circuit breaker
        for _ in 0..3 {
            engine.record_failure("Test").unwrap();
        }
        assert_eq!(engine.circuit_breaker_state().unwrap(), CircuitBreakerState::Open);

        // Check recovery (should transition to HalfOpen immediately)
        let state = engine.check_circuit_breaker_recovery().unwrap();
        assert_eq!(state, CircuitBreakerState::HalfOpen);

        // Confirm recovery
        engine.confirm_recovery().unwrap();
        assert_eq!(engine.circuit_breaker_state().unwrap(), CircuitBreakerState::Closed);
    }

    #[test]
    fn test_update_on_fill_records_pnl() {
        let config = create_test_config();
        let engine = RiskEngine::new(config);

        let pnl = Money::new(dec!(1000), Currency::USD).unwrap();
        engine.update_on_fill(Some(pnl)).unwrap();

        let state = engine.get_state().unwrap();
        assert_eq!(state.daily_realized_pnl.amount(), dec!(1000));
    }

    #[test]
    fn test_update_equity_tracks_peak() {
        let config = create_test_config();
        let engine = RiskEngine::new(config);

        // Set initial equity
        let initial = Money::new(dec!(100_000), Currency::USD).unwrap();
        engine.update_equity(initial).unwrap();

        // Update to higher value
        let higher = Money::new(dec!(110_000), Currency::USD).unwrap();
        engine.update_equity(higher.clone()).unwrap();

        let state = engine.get_state().unwrap();
        assert_eq!(state.equity_peak.amount(), dec!(110_000));
        assert_eq!(state.current_equity.amount(), dec!(110_000));

        // Update to lower value
        let lower = Money::new(dec!(105_000), Currency::USD).unwrap();
        engine.update_equity(lower).unwrap();

        let state = engine.get_state().unwrap();
        assert_eq!(state.equity_peak.amount(), dec!(110_000)); // Peak unchanged
        assert_eq!(state.current_equity.amount(), dec!(105_000));
    }

    #[test]
    fn test_drawdown_limit_blocks_trading() {
        let mut config = create_test_config();
        config.max_drawdown_pct = dec!(0.05); // 5% limit
        let engine = RiskEngine::new(config);

        // Set up equity at peak
        let peak = Money::new(dec!(100_000), Currency::USD).unwrap();
        engine.update_equity(peak).unwrap();

        // Drop 10% (over the 5% limit)
        let low = Money::new(dec!(90_000), Currency::USD).unwrap();
        engine.update_equity(low).unwrap();

        // Try to trade - should be blocked
        let order = create_test_order("AAPL", dec!(10), Side::Buy);
        let positions = vec![];
        let price = Price::new(dec!(150)).unwrap();

        let result = engine.check_pre_trade(&order, &positions, price).unwrap();
        assert!(result.is_rejected());
        assert!(matches!(result, RiskDecision::Reject { reason } if reason.contains("drawdown")));
    }

    #[test]
    fn test_total_exposure_limit() {
        let mut config = create_test_config();
        config.max_total_exposure = Money::new(dec!(50_000), Currency::USD).unwrap();
        let engine = RiskEngine::new(config);

        // Create position that uses most of the limit
        let positions = vec![create_test_position("AAPL", dec!(300), Side::Buy, dec!(150))];

        // Try to add more exposure
        let order = create_test_order("MSFT", dec!(200), Side::Buy);
        let price = Price::new(dec!(200)).unwrap();

        let result = engine.check_pre_trade(&order, &positions, price).unwrap();
        assert!(result.is_rejected());
        assert!(matches!(result, RiskDecision::Reject { reason } if reason.contains("exposure")));
    }

    #[test]
    fn test_symbol_exposure_limit() {
        let mut config = create_test_config();
        config.max_symbol_exposure.insert(
            Symbol::new("AAPL").unwrap(),
            Money::new(dec!(30_000), Currency::USD).unwrap()
        );
        let engine = RiskEngine::new(config);

        // Add existing AAPL position
        let positions = vec![create_test_position("AAPL", dec!(100), Side::Buy, dec!(150))];

        // Try to add more AAPL
        let order = create_test_order("AAPL", dec!(200), Side::Buy);
        let price = Price::new(dec!(150)).unwrap();

        let result = engine.check_pre_trade(&order, &positions, price).unwrap();
        assert!(result.is_rejected());
        assert!(matches!(result, RiskDecision::Reject { reason } if reason.contains("Symbol exposure")));
    }

    #[test]
    fn test_daily_reset_clears_metrics() {
        let config = create_test_config();
        let engine = RiskEngine::new(config);

        // Add some P&L and failures
        let pnl = Money::new(dec!(5000), Currency::USD).unwrap();
        engine.update_on_fill(Some(pnl)).unwrap();
        engine.record_failure("Test").unwrap();

        // Reset
        engine.daily_reset().unwrap();

        let state = engine.get_state().unwrap();
        assert_eq!(state.daily_realized_pnl.amount(), dec!(0));
        assert_eq!(state.consecutive_failures, 0);
    }

    #[test]
    fn test_consecutive_failures_reset_on_success() {
        let config = create_test_config();
        let engine = RiskEngine::new(config);

        // Add some failures
        engine.record_failure("Test1").unwrap();
        engine.record_failure("Test2").unwrap();

        let state = engine.get_state().unwrap();
        assert_eq!(state.consecutive_failures, 2);

        // Success - should reset failures
        engine.update_on_fill(None).unwrap();

        let state = engine.get_state().unwrap();
        assert_eq!(state.consecutive_failures, 0);
    }

    #[test]
    fn test_kill_switch_disabled_in_config() {
        let mut config = create_test_config();
        config.kill_switch_enabled = false;
        let engine = RiskEngine::new(config);

        // Trigger should be no-op
        engine.trigger_kill_switch("Test").unwrap();
        assert!(!engine.is_kill_switch_active().unwrap());
    }

    #[test]
    fn test_risk_state_total_daily_pnl() {
        let mut state = RiskState::default();
        state.daily_realized_pnl = Money::new(dec!(1000), Currency::USD).unwrap();
        state.daily_unrealized_pnl = Money::new(dec!(500), Currency::USD).unwrap();

        assert_eq!(state.total_daily_pnl(), dec!(1500));
    }

    #[test]
    fn test_risk_state_current_drawdown_pct() {
        let mut state = RiskState::default();
        state.equity_peak = Money::new(dec!(100_000), Currency::USD).unwrap();
        state.current_equity = Money::new(dec!(90_000), Currency::USD).unwrap();

        assert_eq!(state.current_drawdown_pct(), dec!(0.10)); // 10% drawdown
    }
}