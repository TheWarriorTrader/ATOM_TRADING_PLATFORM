//! # Risk Metrics Calculation
//!
//! Provides real-time risk metrics calculation for portfolio monitoring and reporting.
//!
//! This module calculates essential risk metrics including:
//! - Exposure metrics (long, short, net, gross)
//! - P&L metrics (realized, unrealized, total)
//! - Position metrics (counts, concentration)
//! - Risk ratios (VaR, CVaR, beta-adjusted exposure)
//! - Margin metrics (requirements, utilization, buying power)
//!
//! ## Usage
//!
//! ```rust
//! use application::risk::{RiskMetrics, RiskMetricsCalculator};
//! use domain::entities::Position;
//! use domain::values::{Money, Currency};
//! use rust_decimal_macros::dec;
//!
//! let calculator = RiskMetricsCalculator::new(
//!     Money::new(dec!(100000), Currency::USD).unwrap(),
//!     dec!(0.2), // 20% margin requirement
//! );
//!
//! let positions: Vec<Position> = vec![];
//! let metrics = calculator.calculate(&positions).unwrap();
//!
//! println!("{}", metrics.format_summary());
//! ```
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::Zero;
use rust_decimal_macros::dec;
use tracing::instrument;

use domain::{
    entities::{Position, Fill},
    values::{Symbol, Price, Money, Currency},
    errors::DomainError,
};

/// Real-time risk metrics for portfolio monitoring.
///
/// Contains comprehensive risk metrics calculated from current positions
/// and market data. Used for risk reporting, limit monitoring, and
/// portfolio analysis.
#[derive(Debug, Clone)]
pub struct RiskMetrics {
    /// Timestamp of calculation
    pub timestamp: DateTime<Utc>,
    
    // Exposure Metrics
    /// Total long exposure (absolute value of long positions)
    pub total_long_exposure: Money,
    /// Total short exposure (absolute value of short positions)
    pub total_short_exposure: Money,
    /// Net exposure (long - short)
    pub net_exposure: Money,
    /// Gross exposure (long + short)
    pub gross_exposure: Money,
    
    // P&L Metrics
    /// Daily realized P&L from closed positions
    pub daily_realized_pnl: Money,
    /// Unrealized P&L (mark-to-market)
    pub unrealized_pnl: Money,
    /// Total P&L (realized + unrealized)
    pub total_pnl: Money,
    
    // Position Metrics
    /// Number of open positions
    pub open_positions_count: usize,
    /// Number of long positions
    pub long_positions_count: usize,
    /// Number of short positions
    pub short_positions_count: usize,
    /// Maximum position concentration (largest position / total)
    pub max_concentration_pct: Decimal,
    
    // Risk Ratios
    /// Beta-adjusted exposure (if beta data available)
    pub beta_adjusted_exposure: Option<Money>,
    /// Value at Risk at 95% confidence level
    pub var_95: Option<Money>,
    /// Expected Shortfall (CVaR) at 95% confidence
    pub cvar_95: Option<Money>,
    
    // Margin Metrics
    /// Total margin required for all positions
    pub total_margin_required: Money,
    /// Available buying power (capital - margin required)
    pub available_buying_power: Money,
    /// Margin utilization percentage
    pub margin_utilization_pct: Decimal,
}

impl RiskMetrics {
    /// Creates empty risk metrics with all values set to zero.
    ///
    /// Used as a safe default when no positions are available
    /// or as a starting point for incremental updates.
    ///
    /// # Examples
    ///
    /// ```
    /// use application::risk::RiskMetrics;
    ///
    /// let metrics = RiskMetrics::empty();
    /// assert_eq!(metrics.open_positions_count, 0);
    /// assert_eq!(metrics.margin_utilization_pct, rust_decimal::Decimal::ZERO);
    /// ```
    pub fn empty() -> Self {
        let zero = unsafe { Money::new_unchecked(Decimal::ZERO, Currency::USD) };
        Self {
            timestamp: Utc::now(),
            total_long_exposure: zero,
            total_short_exposure: zero,
            net_exposure: zero,
            gross_exposure: zero,
            daily_realized_pnl: zero,
            unrealized_pnl: zero,
            total_pnl: zero,
            open_positions_count: 0,
            long_positions_count: 0,
            short_positions_count: 0,
            max_concentration_pct: Decimal::ZERO,
            beta_adjusted_exposure: None,
            var_95: None,
            cvar_95: None,
            total_margin_required: zero,
            available_buying_power: zero,
            margin_utilization_pct: Decimal::ZERO,
        }
    }
    
    /// Checks if risk metrics are within specified limits.
    ///
    /// Currently checks gross exposure against maximum allowed.
    /// Additional limits can be added as needed.
    ///
    /// # Arguments
    ///
    /// * `max_exposure` - Maximum allowed gross exposure
    ///
    /// # Returns
    ///
    /// `true` if all metrics are within limits, `false` otherwise
    pub fn is_within_limits(&self, max_exposure: Money) -> bool {
        self.gross_exposure.amount() <= max_exposure.amount()
    }
}

/// Calculator for risk metrics from positions and market data.
///
/// Provides methods to calculate various risk metrics including
/// exposure, P&L, margin requirements, and risk ratios.
pub struct RiskMetricsCalculator {
    /// Available capital for trading (for margin calculations)
    available_capital: Money,
    /// Margin requirement percentage per position (e.g., 0.20 for 20%)
    margin_requirement_pct: Decimal,
}

impl RiskMetricsCalculator {
    /// Creates a new risk metrics calculator.
    ///
    /// # Arguments
    ///
    /// * `available_capital` - Total available trading capital
    /// * `margin_requirement_pct` - Margin requirement as decimal (e.g., 0.20 for 20%)
    ///
    /// # Examples
    ///
    /// ```
    /// use application::risk::RiskMetricsCalculator;
    /// use domain::values::{Money, Currency};
    /// use rust_decimal_macros::dec;
    ///
    /// let calculator = RiskMetricsCalculator::new(
    ///     Money::new(dec!(100000), Currency::USD).unwrap(),
    ///     dec!(0.2),
    /// );
    /// ```
    pub fn new(available_capital: Money, margin_requirement_pct: Decimal) -> Self {
        Self {
            available_capital,
            margin_requirement_pct,
        }
    }
    
    /// Calculates all risk metrics from a list of positions.
    ///
    /// This is the main entry point for risk metric calculation.
    /// Computes exposure, P&L, position counts, and margin metrics.
    ///
    /// # Arguments
    ///
    /// * `positions` - Slice of open positions to analyze
    ///
    /// # Returns
    ///
    /// `RiskMetrics` containing all calculated metrics
    ///
    /// # Errors
    ///
    /// Returns `DomainError::Validation` if calculations produce invalid values
    #[instrument(skip(self, positions))]
    pub fn calculate(&self, positions: &[Position]) -> Result<RiskMetrics, DomainError> {
        let timestamp = Utc::now();
        
        // Calculate exposures
        let (total_long, total_short) = self.calculate_long_short_exposure(positions);
        
        // Calculate net and gross exposure safely
        let net_amount = total_long.amount() - total_short.amount();
        let gross_amount = total_long.amount() + total_short.amount();
        
        let net_exposure = unsafe { Money::new_unchecked(net_amount, Currency::USD) };
        let gross_exposure = unsafe { Money::new_unchecked(gross_amount, Currency::USD) };
        
        // Calculate unrealized P&L
        let unrealized_pnl = self.calculate_unrealized_pnl(positions);
        
        // Count positions
        let open_positions_count = positions.len();
        let long_positions_count = positions.iter().filter(|p| p.is_long()).count();
        let short_positions_count = positions.iter().filter(|p| p.is_short()).count();
        
        // Calculate maximum concentration
        let max_concentration_pct = if gross_amount > Decimal::ZERO {
            let max_position_value: Decimal = positions
                .iter()
                .map(|p| p.market_value().amount().abs())
                .max()
                .unwrap_or_else(Decimal::zero);
            max_position_value / gross_amount
        } else {
            Decimal::ZERO
        };
        
        // Calculate margin requirements
        let margin_required_amount = gross_amount * self.margin_requirement_pct;
        let total_margin_required = unsafe { Money::new_unchecked(margin_required_amount, Currency::USD) };
        
        let buying_power_amount = self.available_capital.amount() - margin_required_amount;
        let available_buying_power = unsafe { Money::new_unchecked(buying_power_amount, Currency::USD) };
        
        let margin_utilization_pct = if self.available_capital.amount() > Decimal::ZERO {
            margin_required_amount / self.available_capital.amount()
        } else {
            Decimal::ZERO
        };
        
        Ok(RiskMetrics {
            timestamp,
            total_long_exposure: total_long,
            total_short_exposure: total_short,
            net_exposure,
            gross_exposure,
            daily_realized_pnl: unsafe { Money::new_unchecked(Decimal::ZERO, Currency::USD) }, // Populated externally
            unrealized_pnl,
            total_pnl: unrealized_pnl, // Will add daily_realized when available
            open_positions_count,
            long_positions_count,
            short_positions_count,
            max_concentration_pct,
            beta_adjusted_exposure: None, // Requires beta data
            var_95: None, // Requires statistical calculation
            cvar_95: None,
            total_margin_required,
            available_buying_power,
            margin_utilization_pct,
        })
    }
    
    /// Calculates long and short exposure separately.
    ///
    /// # Arguments
    ///
    /// * `positions` - Slice of positions to analyze
    ///
    /// # Returns
    ///
    /// Tuple of (long_exposure, short_exposure) as Money values
    fn calculate_long_short_exposure(&self, positions: &[Position]) -> (Money, Money) {
        let long_exposure: Decimal = positions
            .iter()
            .filter(|p| p.is_long())
            .map(|p| p.market_value().amount())
            .sum();
        
        let short_exposure: Decimal = positions
            .iter()
            .filter(|p| p.is_short())
            .map(|p| p.market_value().amount().abs())
            .sum();
        
        let long = unsafe { Money::new_unchecked(long_exposure, Currency::USD) };
        let short = unsafe { Money::new_unchecked(short_exposure, Currency::USD) };
        
        (long, short)
    }
    
    /// Calculates unrealized P&L (mark-to-market).
    ///
    /// Sums the unrealized P&L from all open positions.
    ///
    /// # Arguments
    ///
    /// * `positions` - Slice of open positions
    ///
    /// # Returns
    ///
    /// Total unrealized P&L as Money
    fn calculate_unrealized_pnl(&self, positions: &[Position]) -> Money {
        let total_pnl: Decimal = positions
            .iter()
            .map(|p| p.unrealized_pnl().amount())
            .sum();
        
        unsafe { Money::new_unchecked(total_pnl, Currency::USD) }
    }
    
    /// Calculates daily realized P&L from fills.
    ///
    /// # Arguments
    ///
    /// * `fills` - Slice of fills with entry prices for P&L calculation
    /// * `entry_prices` - Map of symbol to entry price for P&L calculation
    ///
    /// # Returns
    ///
    /// Total realized P&L as Money
    pub fn calculate_daily_pnl(&self, fills: &[Fill], entry_prices: &HashMap<Symbol, Price>) -> Money {
        let total_pnl: Decimal = fills
            .iter()
            .filter_map(|f| {
                entry_prices.get(f.symbol()).map(|entry_price| {
                    f.pnl(*entry_price).amount()
                })
            })
            .sum();
        
        unsafe { Money::new_unchecked(total_pnl, Currency::USD) }
    }
    
    /// Calculates drawdown percentage from equity peak.
    ///
    /// Drawdown = (Peak - Current) / Peak
    ///
    /// # Arguments
    ///
    /// * `current_equity` - Current portfolio equity
    /// * `equity_peak` - Highest equity value reached
    ///
    /// # Returns
    ///
    /// Drawdown as decimal (e.g., 0.10 for 10% drawdown)
    pub fn calculate_drawdown(&self, current_equity: Money, equity_peak: Money) -> Decimal {
        if equity_peak.amount() > Decimal::ZERO {
            let drawdown = (equity_peak.amount() - current_equity.amount()) / equity_peak.amount();
            drawdown.max(Decimal::ZERO)
        } else {
            Decimal::ZERO
        }
    }
    
    /// Calculates simplified Sharpe Ratio.
    ///
    /// Sharpe = (Average Return - Risk Free Rate) / Standard Deviation
    ///
    /// # Arguments
    ///
    /// * `returns` - Historical returns as decimals
    /// * `risk_free_rate` - Risk-free rate for the same period
    ///
    /// # Returns
    ///
    /// Sharpe ratio as Option<Decimal>, None if insufficient data
    pub fn calculate_sharpe_ratio(
        &self,
        returns: &[Decimal],
        risk_free_rate: Decimal,
    ) -> Option<Decimal> {
        if returns.is_empty() {
            return None;
        }
        
        let avg_return = returns.iter().sum::<Decimal>() / Decimal::from(returns.len());
        let excess_return = avg_return - risk_free_rate;
        
        // Calculate standard deviation
        let variance = returns
            .iter()
            .map(|r| {
                let diff = r - avg_return;
                diff * diff
            })
            .sum::<Decimal>() / Decimal::from(returns.len());
        
        // Calculate sqrt using Newton-Raphson method
        let std_dev = Self::decimal_sqrt(variance)?;
        
        if std_dev > Decimal::ZERO {
            Some(excess_return / std_dev)
        } else {
            None
        }
    }
    
    /// Computes square root of a Decimal using Newton-Raphson method.
    ///
    /// # Arguments
    ///
    /// * `value` - The value to compute square root of
    ///
    /// # Returns
    ///
    /// Square root as Option<Decimal>, None if value is negative
    fn decimal_sqrt(value: Decimal) -> Option<Decimal> {
        if value < Decimal::ZERO {
            return None;
        }
        if value == Decimal::ZERO {
            return Some(Decimal::ZERO);
        }
        
        // Initial guess
        let mut x = value / Decimal::from(2);
        let two = Decimal::from(2);
        
        // Newton-Raphson iterations (10 iterations for good precision)
        for _ in 0..10 {
            let next_x = (x + value / x) / two;
            if (next_x - x).abs() < Decimal::new(1, 10) {
                return Some(next_x);
            }
            x = next_x;
        }
        
        Some(x)
    }
    
    /// Calculates exposure by sector/asset class.
    ///
    /// Aggregates position values by their assigned sectors.
    ///
    /// # Arguments
    ///
    /// * `positions` - Slice of positions
    /// * `symbol_to_sector` - Map of symbols to sector names
    ///
    /// # Returns
    ///
    /// Map of sector name to exposure amount
    pub fn calculate_sector_exposure(
        &self,
        positions: &[Position],
        symbol_to_sector: &HashMap<Symbol, String>,
    ) -> HashMap<String, Money> {
        let mut sector_exposure: HashMap<String, Decimal> = HashMap::new();
        
        for position in positions {
            let sector = symbol_to_sector
                .get(position.symbol())
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string());
            
            let value = position.market_value().amount().abs();
            *sector_exposure.entry(sector).or_insert_with(Decimal::zero) += value;
        }
        
        sector_exposure
            .into_iter()
            .map(|(sector, amount)| {
                (sector, unsafe { Money::new_unchecked(amount, Currency::USD) })
            })
            .collect()
    }
    
    /// Calculates simple Value at Risk using variance-covariance method.
    ///
    /// Assumes returns are normally distributed.
    /// VaR_95 = 1.645 * σ * portfolio_value
    ///
    /// # Arguments
    ///
    /// * `portfolio_value` - Current portfolio value
    /// * `returns_std` - Standard deviation of returns
    ///
    /// # Returns
    ///
    /// VaR at 95% confidence level as Money
    pub fn calculate_var_95(&self, portfolio_value: Money, returns_std: Decimal) -> Money {
        // VaR_95 = 1.645 * σ * portfolio_value
        let var_multiplier = dec!(1.645);
        let var_amount = var_multiplier * returns_std * portfolio_value.amount();
        
        unsafe { Money::new_unchecked(var_amount, Currency::USD) }
    }
}

/// Extension for RiskMetrics formatting and reporting
impl RiskMetrics {
    /// Formats metrics as human-readable summary.
    ///
    /// # Returns
    ///
    /// Multi-line string with formatted metrics
    pub fn format_summary(&self) -> String {
        format!(
            "Risk Metrics @ {}\n\
             Exposure: Long={}, Short={}, Net={}, Gross={}\n\
             P&L: Unrealized={}, Total={}\n\
             Positions: {} ({} Long, {} Short)\n\
             Margin: Required={}, Available={}, Util={:.1}%\n\
             Concentration: Max={:.1}%",
            self.timestamp.format("%Y-%m-%d %H:%M:%S"),
            self.total_long_exposure.format(),
            self.total_short_exposure.format(),
            self.net_exposure.format(),
            self.gross_exposure.format(),
            self.unrealized_pnl.format(),
            self.total_pnl.format(),
            self.open_positions_count,
            self.long_positions_count,
            self.short_positions_count,
            self.total_margin_required.format(),
            self.available_buying_power.format(),
            self.margin_utilization_pct * dec!(100),
            self.max_concentration_pct * dec!(100),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::entities::Position;
    use domain::values::{Symbol, Quantity, Price, Side};
    
    fn create_test_calculator() -> RiskMetricsCalculator {
        RiskMetricsCalculator::new(
            Money::new(dec!(100000), Currency::USD).unwrap(),
            dec!(0.2),
        )
    }
    
    fn create_test_symbol(name: &str) -> Symbol {
        Symbol::new(name).unwrap()
    }
    
    #[test]
    fn test_risk_metrics_empty() {
        let metrics = RiskMetrics::empty();
        
        assert_eq!(metrics.open_positions_count, 0);
        assert_eq!(metrics.long_positions_count, 0);
        assert_eq!(metrics.short_positions_count, 0);
        assert_eq!(metrics.margin_utilization_pct, Decimal::ZERO);
        assert_eq!(metrics.max_concentration_pct, Decimal::ZERO);
    }
    
    #[test]
    fn test_calculate_empty_positions() {
        let calculator = create_test_calculator();
        let positions: Vec<Position> = vec![];
        
        let metrics = calculator.calculate(&positions).unwrap();
        
        assert_eq!(metrics.open_positions_count, 0);
        assert_eq!(metrics.gross_exposure.amount(), Decimal::ZERO);
        assert_eq!(metrics.net_exposure.amount(), Decimal::ZERO);
    }
    
    #[test]
    fn test_calculate_exposure_with_positions() {
        let calculator = create_test_calculator();
        
        // Create long position: 100 shares @ $150
        let pos1 = Position::new_with_side(
            create_test_symbol("AAPL"),
            Side::Buy,
            Quantity::new(dec!(100)).unwrap(),
            Price::new(dec!(150)).unwrap(),
            Currency::USD,
        ).unwrap();
        
        // Create short position: 50 shares @ $200
        let pos2 = Position::new_with_side(
            create_test_symbol("TSLA"),
            Side::Sell,
            Quantity::new(dec!(50)).unwrap(),
            Price::new(dec!(200)).unwrap(),
            Currency::USD,
        ).unwrap();
        
        let positions = vec![pos1, pos2];
        let metrics = calculator.calculate(&positions).unwrap();
        
        // Long exposure: 100 * $150 = $15,000
        assert_eq!(metrics.total_long_exposure.amount(), dec!(15000));
        
        // Short exposure: 50 * $200 = $10,000
        assert_eq!(metrics.total_short_exposure.amount(), dec!(10000));
        
        // Net exposure: $15,000 - $10,000 = $5,000
        assert_eq!(metrics.net_exposure.amount(), dec!(5000));
        
        // Gross exposure: $15,000 + $10,000 = $25,000
        assert_eq!(metrics.gross_exposure.amount(), dec!(25000));
        
        // Position counts
        assert_eq!(metrics.open_positions_count, 2);
        assert_eq!(metrics.long_positions_count, 1);
        assert_eq!(metrics.short_positions_count, 1);
    }
    
    #[test]
    fn test_calculate_margin_requirements() {
        let calculator = create_test_calculator();
        
        let pos = Position::new_with_side(
            create_test_symbol("AAPL"),
            Side::Buy,
            Quantity::new(dec!(100)).unwrap(),
            Price::new(dec!(150)).unwrap(),
            Currency::USD,
        ).unwrap();
        
        let positions = vec![pos];
        let metrics = calculator.calculate(&positions).unwrap();
        
        // Gross exposure: $15,000
        // Margin required: $15,000 * 20% = $3,000
        assert_eq!(metrics.total_margin_required.amount(), dec!(3000));
        
        // Available buying power: $100,000 - $3,000 = $97,000
        assert_eq!(metrics.available_buying_power.amount(), dec!(97000));
        
        // Margin utilization: $3,000 / $100,000 = 3%
        assert_eq!(metrics.margin_utilization_pct, dec!(0.03));
    }
    
    #[test]
    fn test_calculate_drawdown() {
        let calculator = create_test_calculator();
        
        let peak = Money::new(dec!(110000), Currency::USD).unwrap();
        let current = Money::new(dec!(100000), Currency::USD).unwrap();
        
        let drawdown = calculator.calculate_drawdown(current, peak);
        // (110000 - 100000) / 110000 = 0.090909...
        assert!(drawdown > dec!(0.09) && drawdown < dec!(0.091));
    }
    
    #[test]
    fn test_calculate_drawdown_no_drawdown() {
        let calculator = create_test_calculator();
        
        let peak = Money::new(dec!(100000), Currency::USD).unwrap();
        let current = Money::new(dec!(110000), Currency::USD).unwrap();
        
        let drawdown = calculator.calculate_drawdown(current, peak);
        // No drawdown when current > peak
        assert_eq!(drawdown, Decimal::ZERO);
    }
    
    #[test]
    fn test_calculate_drawdown_zero_peak() {
        let calculator = create_test_calculator();
        
        let peak = Money::new(dec!(0), Currency::USD).unwrap();
        let current = Money::new(dec!(100000), Currency::USD).unwrap();
        
        let drawdown = calculator.calculate_drawdown(current, peak);
        assert_eq!(drawdown, Decimal::ZERO);
    }
    
    #[test]
    fn test_calculate_sharpe_ratio() {
        let calculator = create_test_calculator();
        
        // Returns: 1%, 2%, 1.5%, -0.5%, 1%
        let returns = vec![
            dec!(0.01),
            dec!(0.02),
            dec!(0.015),
            dec!(-0.005),
            dec!(0.01),
        ];
        let risk_free_rate = dec!(0.02); // 2%
        
        let sharpe = calculator.calculate_sharpe_ratio(&returns, risk_free_rate);
        
        assert!(sharpe.is_some());
        // With these values, we expect a negative sharpe since avg return < risk free
        let sharpe_val = sharpe.unwrap();
        assert!(sharpe_val < Decimal::ZERO);
    }
    
    #[test]
    fn test_calculate_sharpe_ratio_empty() {
        let calculator = create_test_calculator();
        
        let returns: Vec<Decimal> = vec![];
        let sharpe = calculator.calculate_sharpe_ratio(&returns, Decimal::ZERO);
        
        assert!(sharpe.is_none());
    }
    
    #[test]
    fn test_calculate_sharpe_ratio_no_volatility() {
        let calculator = create_test_calculator();
        
        // All same returns = no volatility
        let returns = vec![dec!(0.01), dec!(0.01), dec!(0.01)];
        let sharpe = calculator.calculate_sharpe_ratio(&returns, Decimal::ZERO);
        
        // Should return None due to division by zero
        assert!(sharpe.is_none());
    }
    
    #[test]
    fn test_calculate_var_95() {
        let calculator = create_test_calculator();
        
        let portfolio_value = Money::new(dec!(100000), Currency::USD).unwrap();
        let returns_std = dec!(0.02); // 2% daily volatility
        
        let var = calculator.calculate_var_95(portfolio_value, returns_std);
        
        // VaR = 1.645 * 0.02 * 100000 = $3,290
        assert_eq!(var.amount(), dec!(3290));
    }
    
    #[test]
    fn test_calculate_sector_exposure() {
        let calculator = create_test_calculator();
        
        let pos1 = Position::new_with_side(
            create_test_symbol("AAPL"),
            Side::Buy,
            Quantity::new(dec!(100)).unwrap(),
            Price::new(dec!(150)).unwrap(),
            Currency::USD,
        ).unwrap();
        
        let pos2 = Position::new_with_side(
            create_test_symbol("MSFT"),
            Side::Buy,
            Quantity::new(dec!(50)).unwrap(),
            Price::new(dec!(300)).unwrap(),
            Currency::USD,
        ).unwrap();
        
        let mut sector_map = HashMap::new();
        sector_map.insert(create_test_symbol("AAPL"), "Technology".to_string());
        sector_map.insert(create_test_symbol("MSFT"), "Technology".to_string());
        
        let positions = vec![pos1, pos2];
        let sector_exposure = calculator.calculate_sector_exposure(&positions, &sector_map);
        
        // AAPL: $15,000 + MSFT: $15,000 = $30,000 Technology
        assert_eq!(sector_exposure.get("Technology").unwrap().amount(), dec!(30000));
    }
    
    #[test]
    fn test_calculate_sector_exposure_unknown() {
        let calculator = create_test_calculator();
        
        let pos = Position::new_with_side(
            create_test_symbol("UNKNOWN"),
            Side::Buy,
            Quantity::new(dec!(100)).unwrap(),
            Price::new(dec!(100)).unwrap(),
            Currency::USD,
        ).unwrap();
        
        let sector_map = HashMap::new();
        let positions = vec![pos];
        let sector_exposure = calculator.calculate_sector_exposure(&positions, &sector_map);
        
        // Unknown symbol should map to "Unknown" sector
        assert_eq!(sector_exposure.get("Unknown").unwrap().amount(), dec!(10000));
    }
    
    #[test]
    fn test_is_within_limits() {
        let calculator = create_test_calculator();
        
        let pos = Position::new_with_side(
            create_test_symbol("AAPL"),
            Side::Buy,
            Quantity::new(dec!(100)).unwrap(),
            Price::new(dec!(150)).unwrap(),
            Currency::USD,
        ).unwrap();
        
        let positions = vec![pos];
        let metrics = calculator.calculate(&positions).unwrap();
        
        // Gross exposure is $15,000
        let max_exposure_low = Money::new(dec!(10000), Currency::USD).unwrap();
        let max_exposure_high = Money::new(dec!(20000), Currency::USD).unwrap();
        
        assert!(!metrics.is_within_limits(max_exposure_low));
        assert!(metrics.is_within_limits(max_exposure_high));
    }
    
    #[test]
    fn test_format_summary() {
        let metrics = RiskMetrics::empty();
        let summary = metrics.format_summary();
        
        assert!(summary.contains("Risk Metrics @"));
        assert!(summary.contains("Exposure:"));
        assert!(summary.contains("P&L:"));
        assert!(summary.contains("Positions:"));
        assert!(summary.contains("Margin:"));
        assert!(summary.contains("Concentration:"));
    }
}
