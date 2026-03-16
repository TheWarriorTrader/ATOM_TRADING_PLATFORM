//! # Trading Commands
//!
//! CLI subcommands for interacting with the trading service.
//! Provides order management, position tracking, and order status monitoring.
//!
//! ## Commands
//!
//! - `place-order`: Submit a new order to the market
//! - `cancel-order`: Cancel an existing order
//! - `get-positions`: List current trading positions
//! - `get-orders`: List orders with optional filtering
//!
//! ## Examples
//!
//! ```bash
//! # Place a market order
//! cargo run --bin cli -- place-order --symbol NQ --side Buy --quantity 1 --type Market
//!
//! # Place a limit order
//! cargo run --bin cli -- place-order --symbol ES --side Sell --quantity 2 --type Limit --price 5200.00
//!
//! # Cancel an order
//! cargo run --bin cli -- cancel-order --id ord-abc123
//!
//! # List all positions
//! cargo run --bin cli -- get-positions
//!
//! # List positions for specific symbol
//! cargo run --bin cli -- get-positions --symbol NQ
//!
//! # List open orders
//! cargo run --bin cli -- get-orders --status open
//! ```

use std::sync::Arc;
use std::str::FromStr;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rust_decimal::Decimal;
use tracing::{info, error, instrument};
use uuid::Uuid;

use application::trading_service::{TradingService, TradingError, EventBusPort};
use application::risk::{RiskEngine, RiskConfig};
use domain::entities::{Order, OrderType, OrderStatus, EntityId};
use domain::events::TradingEvent;
use domain::values::{Symbol, Side, Quantity, Price, OrderId, TimeInForce};
use domain::repositories::{OrderRepository, PositionRepository, BarRepository};
use domain::execution::ExecutionGateway;
use domain::errors::{DomainError, RepositoryError};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use tokio::sync::RwLock;

// =============================================================================
// EVENT BUS PORT IMPLEMENTATION
// =============================================================================

/// Adapter that implements EventBusPort for the infrastructure EventBus.
pub struct EventBusAdapter {
    inner: EventBus,
}

impl EventBusAdapter {
    /// Creates a new EventBusAdapter wrapping the given EventBus.
    pub fn new(event_bus: EventBus) -> Self {
        Self { inner: event_bus }
    }
}

impl Default for EventBusAdapter {
    fn default() -> Self {
        Self::new(EventBus::default())
    }
}

#[async_trait]
impl EventBusPort for EventBusAdapter {
    async fn publish(&self, event: TradingEvent) -> usize {
        // EventBus.publish is synchronous, so we call it directly
        self.inner.publish(event)
    }
}

// =============================================================================
// MOCK REPOSITORIES
// =============================================================================

/// In-memory mock order repository for CLI usage.
/// This is used when database connection is not available.
#[derive(Debug, Default)]
pub struct MockOrderRepository {
    orders: RwLock<HashMap<OrderId, Order>>,
}

impl MockOrderRepository {
    /// Creates a new empty mock order repository.
    pub fn new() -> Self {
        Self {
            orders: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl OrderRepository for MockOrderRepository {
    async fn save(&self, order: &Order) -> Result<(), RepositoryError> {
        let mut orders = self.orders.write().await;
        orders.insert(OrderId::from_uuid(order.id()), order.clone());
        Ok(())
    }

    async fn update(&self, order: &Order) -> Result<(), RepositoryError> {
        let mut orders = self.orders.write().await;
        orders.insert(OrderId::from_uuid(order.id()), order.clone());
        Ok(())
    }

    async fn get(&self, order_id: OrderId) -> Result<Option<Order>, RepositoryError> {
        let orders = self.orders.read().await;
        Ok(orders.get(&order_id).cloned())
    }

    async fn get_open(&self) -> Result<Vec<Order>, RepositoryError> {
        let orders = self.orders.read().await;
        Ok(orders
            .values()
            .filter(|o| o.status().is_active())
            .cloned()
            .collect())
    }

    async fn get_by_symbol(&self, symbol: Symbol) -> Result<Vec<Order>, RepositoryError> {
        let orders = self.orders.read().await;
        Ok(orders
            .values()
            .filter(|o| o.symbol() == &symbol)
            .cloned()
            .collect())
    }

    async fn get_by_account(&self, account_id: EntityId) -> Result<Vec<Order>, RepositoryError> {
        let orders = self.orders.read().await;
        Ok(orders
            .values()
            .filter(|o| o.account_id() == account_id)
            .cloned()
            .collect())
    }

    async fn get_history(
        &self,
        _start: DateTime<Utc>,
        _end: DateTime<Utc>,
    ) -> Result<Vec<Order>, RepositoryError> {
        let orders = self.orders.read().await;
        Ok(orders.values().cloned().collect())
    }

    async fn delete(&self, order_id: OrderId) -> Result<(), RepositoryError> {
        let mut orders = self.orders.write().await;
        orders.remove(&order_id);
        Ok(())
    }

    async fn count_by_status(&self) -> Result<Vec<(String, i64)>, RepositoryError> {
        Ok(vec![])
    }
}

/// In-memory mock position repository for CLI usage.
#[derive(Debug, Default)]
pub struct MockPositionRepository {
    positions: RwLock<HashMap<Symbol, Position>>,
}

impl MockPositionRepository {
    /// Creates a new empty mock position repository.
    pub fn new() -> Self {
        Self {
            positions: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl PositionRepository for MockPositionRepository {
    async fn save(&self, position: &Position) -> Result<(), RepositoryError> {
        let mut positions = self.positions.write().await;
        positions.insert(position.symbol().clone(), position.clone());
        Ok(())
    }

    async fn upsert(&self, position: &Position) -> Result<(), RepositoryError> {
        let mut positions = self.positions.write().await;
        positions.insert(position.symbol().clone(), position.clone());
        Ok(())
    }

    async fn find_by_id(&self, _id: EntityId) -> Result<Option<Position>, RepositoryError> {
        Ok(None)
    }

    async fn find_by_symbol(&self, symbol: Symbol) -> Result<Option<Position>, RepositoryError> {
        let positions = self.positions.read().await;
        Ok(positions.get(&symbol).cloned())
    }

    async fn find_open_positions(&self) -> Result<Vec<Position>, RepositoryError> {
        let positions = self.positions.read().await;
        Ok(positions.values().cloned().collect())
    }

    async fn find_by_account_and_symbol(
        &self,
        _account_id: EntityId,
        symbol: Symbol,
    ) -> Result<Option<Position>, RepositoryError> {
        let positions = self.positions.read().await;
        Ok(positions.get(&symbol).cloned())
    }

    async fn close(&self, _id: EntityId) -> Result<(), RepositoryError> {
        Ok(())
    }

    async fn delete(&self, _id: EntityId) -> Result<(), RepositoryError> {
        Ok(())
    }

    async fn exists(&self, _id: EntityId) -> Result<bool, RepositoryError> {
        Ok(false)
    }

    async fn total_pnl(&self) -> Result<domain::values::Money, RepositoryError> {
        Err(RepositoryError::query("Not implemented"))
    }

    async fn total_exposure(&self) -> Result<domain::values::Money, RepositoryError> {
        Err(RepositoryError::query("Not implemented"))
    }
}
use infrastructure::config::AppConfig;
use infrastructure::messaging::EventBus;

// =============================================================================
// TRADING CLI ARGUMENTS
// =============================================================================

/// Trading commands for order and position management
#[derive(Parser, Debug)]
pub struct TradingCommand {
    #[command(subcommand)]
    pub command: TradingSubcommand,
}

/// Available trading subcommands
#[derive(Subcommand, Debug)]
pub enum TradingSubcommand {
    /// Place a new order
    ///
    /// Creates and submits a new order to the market.
    /// Supports Market, Limit, Stop, and Stop-Limit order types.
    ///
    /// # Examples
    ///
    /// ```bash
    /// # Market order
    /// cargo run --bin cli -- trading place-order --symbol NQ --side Buy --quantity 1
    ///
    /// # Limit order
    /// cargo run --bin cli -- trading place-order --symbol ES --side Sell --quantity 2 --type Limit --price 5200.00
    ///
    /// # Stop order
    /// cargo run --bin cli -- trading place-order --symbol NQ --side Sell --quantity 1 --type Stop --stop-price 18000.00
    /// ```
    PlaceOrder(PlaceOrderArgs),

    /// Cancel an existing order
    ///
    /// Cancels an order that is still active (Created, Submitted, Pending, or PartiallyFilled).
    ///
    /// # Examples
    ///
    /// ```bash
    /// cargo run --bin cli -- trading cancel-order --id ord-abc123
    /// ```
    CancelOrder(CancelOrderArgs),

    /// Get current positions
    ///
    /// Lists all open positions with P&L and exposure information.
    ///
    /// # Examples
    ///
    /// ```bash
    /// # All positions
    /// cargo run --bin cli -- trading get-positions
    ///
    /// # Filter by symbol
    /// cargo run --bin cli -- trading get-positions --symbol NQ
    /// ```
    GetPositions(GetPositionsArgs),

    /// Get orders
    ///
    /// Lists orders with optional filtering by status and symbol.
    ///
    /// # Examples
    ///
    /// ```bash
    /// # All orders
    /// cargo run --bin cli -- trading get-orders
    ///
    /// # Open orders only
    /// cargo run --bin cli -- trading get-orders --status open
    ///
    /// # Orders for specific symbol
    /// cargo run --bin cli -- trading get-orders --symbol NQ --status filled
    /// ```
    GetOrders(GetOrdersArgs),
}

/// Arguments for placing a new order
#[derive(Parser, Debug, Clone)]
pub struct PlaceOrderArgs {
    /// Trading symbol (e.g., NQ, ES, AAPL)
    #[arg(short, long, required = true, help = "Trading symbol (e.g., NQ, ES, AAPL)")]
    pub symbol: String,

    /// Order side (Buy or Sell)
    #[arg(short, long, required = true, value_parser = parse_side, help = "Order side: Buy or Sell")]
    pub side: Side,

    /// Order quantity
    #[arg(short, long, required = true, value_parser = parse_quantity, help = "Order quantity (must be positive)")]
    pub quantity: Quantity,

    /// Order type (Market, Limit, Stop, StopLimit)
    #[arg(short = 't', long, default_value = "Market", value_parser = parse_order_type, help = "Order type: Market, Limit, Stop, StopLimit")]
    pub order_type: OrderType,

    /// Limit price (required for Limit and StopLimit orders)
    #[arg(long, value_parser = parse_price, help = "Limit price (required for Limit/StopLimit)")]
    pub price: Option<Price>,

    /// Stop price (required for Stop and StopLimit orders)
    #[arg(long, value_parser = parse_price, help = "Stop price (required for Stop/StopLimit)")]
    pub stop_price: Option<Price>,

    /// Time in force (Day, GTC, IOC, FOK)
    #[arg(long, default_value = "Day", value_parser = parse_time_in_force, help = "Time in force: Day, GTC, IOC, FOK")]
    pub tif: TimeInForce,

    /// Account ID (optional, uses default if not provided)
    #[arg(long, help = "Account ID (UUID format)")]
    pub account_id: Option<String>,
}

/// Arguments for canceling an order
#[derive(Parser, Debug, Clone)]
pub struct CancelOrderArgs {
    /// Order ID to cancel
    #[arg(short, long, required = true, help = "Order ID (e.g., ord-abc123)")]
    pub id: String,
}

/// Arguments for getting positions
#[derive(Parser, Debug, Clone)]
pub struct GetPositionsArgs {
    /// Filter by symbol
    #[arg(short, long, help = "Filter positions by symbol")]
    pub symbol: Option<String>,
}

/// Arguments for getting orders
#[derive(Parser, Debug, Clone)]
pub struct GetOrdersArgs {
    /// Filter by status (open, filled, cancelled, rejected, all)
    #[arg(short, long, value_parser = parse_order_status_filter, help = "Filter by status: open, filled, cancelled, rejected, all")]
    pub status: Option<OrderStatusFilter>,

    /// Filter by symbol
    #[arg(short, long, help = "Filter orders by symbol")]
    pub symbol: Option<String>,
}

// =============================================================================
// PARSER HELPERS
// =============================================================================

/// Filter options for order status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatusFilter {
    Open,
    Filled,
    Cancelled,
    Rejected,
    All,
}

impl OrderStatusFilter {
    /// Returns true if this filter matches the given order status
    pub fn matches(&self, status: &OrderStatus) -> bool {
        match self {
            Self::Open => status.is_active(),
            Self::Filled => matches!(status, OrderStatus::Filled { .. }),
            Self::Cancelled => matches!(status, OrderStatus::Cancelled { .. }),
            Self::Rejected => matches!(status, OrderStatus::Rejected { .. }),
            Self::All => true,
        }
    }
}

impl FromStr for OrderStatusFilter {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_order_status_filter(s)
    }
}

fn parse_order_status_filter(s: &str) -> Result<OrderStatusFilter, String> {
    match s.to_lowercase().as_str() {
        "open" => Ok(OrderStatusFilter::Open),
        "filled" => Ok(OrderStatusFilter::Filled),
        "cancelled" | "canceled" => Ok(OrderStatusFilter::Cancelled),
        "rejected" => Ok(OrderStatusFilter::Rejected),
        "all" => Ok(OrderStatusFilter::All),
        _ => Err(format!("Invalid status filter: {}. Use: open, filled, cancelled, rejected, all", s)),
    }
}

fn parse_side(s: &str) -> Result<Side, String> {
    Side::parse(s).map_err(|e| e.to_string())
}

fn parse_quantity(s: &str) -> Result<Quantity, String> {
    let decimal = Decimal::from_str(s)
        .map_err(|e| format!("Invalid quantity '{}': {}", s, e))?;
    Quantity::new(decimal)
        .map_err(|e| e.to_string())
}

fn parse_price(s: &str) -> Result<Price, String> {
    Price::from_str(s).map_err(|e| e.to_string())
}

fn parse_order_type(s: &str) -> Result<OrderType, String> {
    OrderType::parse(s).map_err(|e| e.to_string())
}

fn parse_time_in_force(s: &str) -> Result<TimeInForce, String> {
    TimeInForce::parse(s).map_err(|e| e.to_string())
}

// =============================================================================
// COMMAND EXECUTION
// =============================================================================

impl TradingCommand {
    /// Execute the trading command
    #[instrument(skip(self, config), fields(command = ?self.command))]
    pub async fn execute(&self, config: &AppConfig) -> Result<()> {
        match &self.command {
            TradingSubcommand::PlaceOrder(args) => execute_place_order(args, config).await,
            TradingSubcommand::CancelOrder(args) => execute_cancel_order(args, config).await,
            TradingSubcommand::GetPositions(args) => execute_get_positions(args, config).await,
            TradingSubcommand::GetOrders(args) => execute_get_orders(args, config).await,
        }
    }
}

/// Initialize trading service with all dependencies
async fn init_trading_service(_config: &AppConfig) -> Result<TradingService> {
    info!("Initializing trading service dependencies...");

    // Create mock repositories for CLI usage
    // In production, these would be TimescaleDB repositories
    let order_repo: Arc<dyn OrderRepository> = Arc::new(MockOrderRepository::new());
    let position_repo: Arc<dyn PositionRepository> = Arc::new(MockPositionRepository::new());

    // Create execution gateway (mock for CLI)
    let execution = create_mock_execution_gateway().await?;

    // Create risk engine with default config
    let risk_config = RiskConfig::default();
    let risk_engine = RiskEngine::new(risk_config);

    // Create event bus
    let event_bus: Arc<dyn EventBusPort> = Arc::new(EventBusAdapter::default());

    info!("Trading service initialized successfully");

    Ok(TradingService::new(
        execution,
        order_repo,
        position_repo,
        risk_engine,
        event_bus,
    ))
}

/// Create mock execution gateway for CLI usage
async fn create_mock_execution_gateway() -> Result<Arc<dyn ExecutionGateway>> {
    // Use mock execution gateway for CLI operations
    info!("Using mock execution gateway");
    
    let gateway = infrastructure::external::MockOrderExecutionAdapter::new(
        infrastructure::external::HttpClient::new("https://api.example.com", None),
    );
    
    Ok(Arc::new(gateway))
}

/// Execute place-order command
#[instrument(skip(args, config))]
async fn execute_place_order(args: &PlaceOrderArgs, config: &AppConfig) -> Result<()> {
    info!("Executing place-order command");

    // Validate arguments
    validate_place_order_args(args)?;

    // Initialize trading service
    let service = init_trading_service(config).await?;

    // Parse symbol
    let symbol = Symbol::new(&args.symbol)
        .with_context(|| format!("Invalid symbol: {}", args.symbol))?;

    // Get or generate account ID
    let account_id = match &args.account_id {
        Some(id_str) => Uuid::parse_str(id_str)
            .with_context(|| format!("Invalid account ID: {}", id_str))?,
        None => Uuid::new_v4(), // Generate new account ID if not provided
    };

    // Create order
    let order = Order::new(
        account_id,
        symbol,
        args.side,
        args.order_type,
        args.quantity,
        args.price,
        args.stop_price,
    ).with_context(|| "Failed to create order: validation error")?;

    info!(
        symbol = %args.symbol,
        side = ?args.side,
        quantity = %args.quantity,
        order_type = ?args.order_type,
        "Placing order"
    );

    // Place order through service
    match service.place_order(order).await {
        Ok(order_id) => {
            print_order_placed_success(&order_id, args);
            Ok(())
        }
        Err(e) => {
            error!("Order placement failed: {}", e);
            print_trading_error(&e);
            Err(anyhow::anyhow!("Order placement failed: {}", e))
        }
    }
}

/// Validate place-order arguments
fn validate_place_order_args(args: &PlaceOrderArgs) -> Result<()> {
    // Validate limit orders have price
    if args.order_type.requires_limit_price() && args.price.is_none() {
        anyhow::bail!("Limit orders require a --price argument");
    }

    // Validate stop orders have stop price
    if args.order_type.requires_stop_price() && args.stop_price.is_none() {
        anyhow::bail!("Stop orders require a --stop-price argument");
    }

    Ok(())
}

/// Execute cancel-order command
#[instrument(skip(args, config))]
async fn execute_cancel_order(args: &CancelOrderArgs, config: &AppConfig) -> Result<()> {
    info!(order_id = %args.id, "Executing cancel-order command");

    // Parse order ID
    let order_id = OrderId::from_str(&args.id)
        .with_context(|| format!("Invalid order ID: {}", args.id))?;

    // Initialize trading service
    let service = init_trading_service(config).await?;

    // Cancel order
    match service.cancel_order(order_id).await {
        Ok(()) => {
            println!("✅ Order cancelled successfully");
            println!("   Order ID: {}", order_id);
            Ok(())
        }
        Err(e) => {
            error!("Order cancellation failed: {}", e);
            print_trading_error(&e);
            Err(anyhow::anyhow!("Order cancellation failed: {}", e))
        }
    }
}

/// Execute get-positions command
#[instrument(skip(args, config))]
async fn execute_get_positions(args: &GetPositionsArgs, config: &AppConfig) -> Result<()> {
    info!("Executing get-positions command");

    // Initialize trading service
    let service = init_trading_service(config).await?;

    // Get positions
    let positions = service.get_positions().await
        .with_context(|| "Failed to retrieve positions")?;

    // Filter by symbol if specified
    let filtered_positions: Vec<_> = match &args.symbol {
        Some(symbol_str) => {
            let filter_symbol = Symbol::new(symbol_str)
                .with_context(|| format!("Invalid symbol: {}", symbol_str))?;
            positions.into_iter()
                .filter(|p| p.symbol() == &filter_symbol)
                .collect()
        }
        None => positions,
    };

    // Print positions table
    print_positions_table(&filtered_positions);

    Ok(())
}

/// Execute get-orders command
#[instrument(skip(args, config))]
async fn execute_get_orders(args: &GetOrdersArgs, config: &AppConfig) -> Result<()> {
    info!("Executing get-orders command");

    // Initialize trading service
    let service = init_trading_service(config).await?;

    // Get orders based on status filter
    let orders = if let Some(filter) = args.status {
        match filter {
            OrderStatusFilter::Open => service.get_open_orders().await
                .with_context(|| "Failed to retrieve open orders")?,
            _ => {
                // For other filters, get all orders and filter locally
                // This is a simplification - in production, use repository directly
                let all_orders = service.get_open_orders().await
                    .with_context(|| "Failed to retrieve orders")?;
                all_orders.into_iter()
                    .filter(|o| filter.matches(&o.status()))
                    .collect()
            }
        }
    } else {
        // Default to open orders
        service.get_open_orders().await
            .with_context(|| "Failed to retrieve orders")?
    };

    // Filter by symbol if specified
    let filtered_orders: Vec<_> = match &args.symbol {
        Some(symbol_str) => {
            let filter_symbol = Symbol::new(symbol_str)
                .with_context(|| format!("Invalid symbol: {}", symbol_str))?;
            orders.into_iter()
                .filter(|o| o.symbol() == &filter_symbol)
                .collect()
        }
        None => orders,
    };

    // Print orders table
    print_orders_table(&filtered_orders);

    Ok(())
}

// =============================================================================
// OUTPUT FORMATTING
// =============================================================================

/// Print success message for placed order
fn print_order_placed_success(order_id: &OrderId, args: &PlaceOrderArgs) {
    println!("✅ Order placed successfully");
    println!();
    println!("Order Details:");
    println!("  Order ID:    {}", order_id);
    println!("  Symbol:      {}", args.symbol);
    println!("  Side:        {:?}", args.side);
    println!("  Quantity:    {}", args.quantity);
    println!("  Type:        {:?}", args.order_type);
    if let Some(price) = args.price {
        println!("  Limit Price: {:.2}", price.inner());
    }
    if let Some(stop_price) = args.stop_price {
        println!("  Stop Price:  {:.2}", stop_price.inner());
    }
    println!("  TIF:         {:?}", args.tif);
}

/// Print trading error with user-friendly message
fn print_trading_error(error: &TradingError) {
    eprintln!("❌ Error: {}", error);
    
    match error {
        TradingError::Validation { message } => {
            eprintln!("   The order parameters are invalid. Please check:");
            eprintln!("   - Symbol is valid (e.g., NQ, ES, AAPL)");
            eprintln!("   - Quantity is positive");
            eprintln!("   - Price is provided for Limit/StopLimit orders");
            eprintln!("   - Stop price is provided for Stop/StopLimit orders");
            eprintln!("   Details: {}", message);
        }
        TradingError::RiskRejected { reason } => {
            eprintln!("   The order was rejected by risk management.");
            eprintln!("   Reason: {}", reason);
        }
        TradingError::Execution { message } => {
            eprintln!("   The order could not be executed.");
            eprintln!("   Details: {}", message);
        }
        TradingError::OrderNotFound { order_id } => {
            eprintln!("   The specified order was not found.");
            eprintln!("   Order ID: {}", order_id);
        }
        TradingError::InvalidState { message } => {
            eprintln!("   The order is in an invalid state for this operation.");
            eprintln!("   Details: {}", message);
        }
        TradingError::CancellationFailed { message } => {
            eprintln!("   The order could not be cancelled.");
            eprintln!("   Details: {}", message);
        }
        TradingError::Repository { message } => {
            eprintln!("   A database error occurred.");
            eprintln!("   Details: {}", message);
        }
    }
}

/// Print positions in table format
fn print_positions_table(positions: &[domain::entities::Position]) {
    if positions.is_empty() {
        println!("No open positions found.");
        return;
    }

    // Calculate column widths
    let symbol_width = positions.iter()
        .map(|p| p.symbol().as_str().len())
        .max()
        .unwrap_or(6)
        .max(6);

    // Header
    print_table_separator(symbol_width);
    println!("| {:^width$} | {:>8} | {:>12} | {:>12} | {:>10} |",
             "Symbol", "Quantity", "Avg Price", "P&L", "Side",
             width = symbol_width);
    print_table_separator(symbol_width);

    // Rows
    for position in positions {
        let symbol = position.symbol().as_str();
        let quantity = position.quantity();
        let avg_price = position.average_entry_price();
        let unrealized_pnl = position.unrealized_pnl();
        let side = if quantity.inner() > Decimal::ZERO { "Long" } else { "Short" };

        println!("| {:>width$} | {:>8.2} | {:>12.2} | {:>12.2} | {:>10} |",
            symbol,
            quantity.inner().abs(),
            avg_price.inner(),
            unrealized_pnl.map(|p| p.amount()).unwrap_or(Decimal::ZERO),
            side,
            width = symbol_width
        );
    }

    print_table_separator(symbol_width);
    println!("Total positions: {}", positions.len());
}

/// Print orders in table format
fn print_orders_table(orders: &[Order]) {
    if orders.is_empty() {
        println!("No orders found.");
        return;
    }

    // Calculate column widths
    let id_width = orders.iter()
        .map(|o| o.id().to_string().len())
        .max()
        .unwrap_or(10)
        .max(10);
    let symbol_width = orders.iter()
        .map(|o| o.symbol().as_str().len())
        .max()
        .unwrap_or(6)
        .max(6);

    // Header
    print_orders_table_separator(id_width, symbol_width);
    println!("| {:^id_width$} | {:^symbol_width$} | {:>6} | {:>10} | {:>12} | {:>16} |",
             "Order ID", "Symbol", "Side", "Quantity", "Status", "Timestamp",
             id_width = id_width, symbol_width = symbol_width);
    print_orders_table_separator(id_width, symbol_width);

    // Rows
    for order in orders {
        let id = format!("{}", order.id());
        let symbol = order.symbol().as_str();
        let side = format!("{:?}", order.side());
        let quantity = order.quantity();
        let status = order.status().variant_name();
        let timestamp = order.created_at().format("%Y-%m-%d %H:%M");

        println!("| {:>id_width$} | {:>symbol_width$} | {:>6} | {:>10.2} | {:>12} | {:>16} |",
            &id[..id.len().min(id_width)],
            symbol,
            side,
            quantity.inner(),
            status,
            timestamp,
            id_width = id_width,
            symbol_width = symbol_width
        );
    }

    print_orders_table_separator(id_width, symbol_width);
    println!("Total orders: {}", orders.len());
}

/// Print separator line for positions table
fn print_table_separator(symbol_width: usize) {
    let total_width = symbol_width + 8 + 12 + 12 + 10 + 16; // columns + separators
    println!("{}", "-".repeat(total_width));
}

/// Print separator line for orders table
fn print_orders_table_separator(id_width: usize, symbol_width: usize) {
    let total_width = id_width + symbol_width + 6 + 10 + 12 + 16 + 18; // columns + separators
    println!("{}", "-".repeat(total_width));
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    // =========================================================================
    // PARSER TESTS
    // =========================================================================

    #[test]
    fn test_parse_side_buy() {
        let result = parse_side("buy");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Side::Buy);
    }

    #[test]
    fn test_parse_side_sell() {
        let result = parse_side("SELL");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Side::Sell);
    }

    #[test]
    fn test_parse_side_invalid() {
        let result = parse_side("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_quantity_valid() {
        let result = parse_quantity("100.5");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().inner(), dec!(100.5));
    }

    #[test]
    fn test_parse_quantity_zero() {
        let result = parse_quantity("0");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_quantity_negative() {
        let result = parse_quantity("-10");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_price_valid() {
        let result = parse_price("150.25");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_price_negative() {
        let result = parse_price("-100");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_order_type_market() {
        let result = parse_order_type("market");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), OrderType::Market);
    }

    #[test]
    fn test_parse_order_type_limit() {
        let result = parse_order_type("LIMIT");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), OrderType::Limit);
    }

    #[test]
    fn test_parse_order_type_invalid() {
        let result = parse_order_type("unknown");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_time_in_force_day() {
        let result = parse_time_in_force("day");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), TimeInForce::Day);
    }

    #[test]
    fn test_parse_time_in_force_gtc() {
        let result = parse_time_in_force("GTC");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), TimeInForce::GTC);
    }

    #[test]
    fn test_parse_order_status_filter() {
        assert_eq!(parse_order_status_filter("open").unwrap(), OrderStatusFilter::Open);
        assert_eq!(parse_order_status_filter("filled").unwrap(), OrderStatusFilter::Filled);
        assert_eq!(parse_order_status_filter("cancelled").unwrap(), OrderStatusFilter::Cancelled);
        assert_eq!(parse_order_status_filter("all").unwrap(), OrderStatusFilter::All);
    }

    #[test]
    fn test_parse_order_status_filter_invalid() {
        assert!(parse_order_status_filter("unknown").is_err());
    }

    // =========================================================================
    // VALIDATION TESTS
    // =========================================================================

    #[test]
    fn test_validate_place_order_args_market() {
        let args = PlaceOrderArgs {
            symbol: "NQ".to_string(),
            side: Side::Buy,
            quantity: Quantity::new(dec!(1)).unwrap(),
            order_type: OrderType::Market,
            price: None,
            stop_price: None,
            tif: TimeInForce::Day,
            account_id: None,
        };
        assert!(validate_place_order_args(&args).is_ok());
    }

    #[test]
    fn test_validate_place_order_args_limit_without_price() {
        let args = PlaceOrderArgs {
            symbol: "ES".to_string(),
            side: Side::Sell,
            quantity: Quantity::new(dec!(2)).unwrap(),
            order_type: OrderType::Limit,
            price: None,
            stop_price: None,
            tif: TimeInForce::Day,
            account_id: None,
        };
        assert!(validate_place_order_args(&args).is_err());
    }

    #[test]
    fn test_validate_place_order_args_limit_with_price() {
        let args = PlaceOrderArgs {
            symbol: "ES".to_string(),
            side: Side::Sell,
            quantity: Quantity::new(dec!(2)).unwrap(),
            order_type: OrderType::Limit,
            price: Some(Price::new(dec!(5200)).unwrap()),
            stop_price: None,
            tif: TimeInForce::Day,
            account_id: None,
        };
        assert!(validate_place_order_args(&args).is_ok());
    }

    #[test]
    fn test_validate_place_order_args_stop_without_stop_price() {
        let args = PlaceOrderArgs {
            symbol: "NQ".to_string(),
            side: Side::Sell,
            quantity: Quantity::new(dec!(1)).unwrap(),
            order_type: OrderType::Stop,
            price: None,
            stop_price: None,
            tif: TimeInForce::Day,
            account_id: None,
        };
        assert!(validate_place_order_args(&args).is_err());
    }

    // =========================================================================
    // FILTER TESTS
    // =========================================================================

    #[test]
    fn test_order_status_filter_matches_open() {
        use chrono::Utc;
        
        let filter = OrderStatusFilter::Open;
        assert!(filter.matches(&OrderStatus::Created));
        assert!(filter.matches(&OrderStatus::Submitted { at: Utc::now() }));
        assert!(!filter.matches(&OrderStatus::Filled { at: Utc::now() }));
    }

    #[test]
    fn test_order_status_filter_matches_filled() {
        use chrono::Utc;
        
        let filter = OrderStatusFilter::Filled;
        assert!(filter.matches(&OrderStatus::Filled { at: Utc::now() }));
        assert!(!filter.matches(&OrderStatus::Created));
    }

    #[test]
    fn test_order_status_filter_matches_all() {
        use chrono::Utc;
        
        let filter = OrderStatusFilter::All;
        assert!(filter.matches(&OrderStatus::Created));
        assert!(filter.matches(&OrderStatus::Filled { at: Utc::now() }));
        assert!(filter.matches(&OrderStatus::Cancelled { at: Utc::now(), reason: "test".to_string() }));
    }
}
