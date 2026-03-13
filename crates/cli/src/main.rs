//! # Trading Core CLI
//!
//! Command-line interface for the trading platform.
//! This is the binary entry point that depends on all other crates.

use std::sync::Arc;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::{info, warn};

use application::{
    account_use_cases::GetAccountsUseCase,
    dto::{CreateOrderDto, OrderSideDto, OrderTypeDto},
    order_use_cases::CreateOrderUseCase,
    ports::{MarketDataPort, NotificationLevel, NotificationPort, OrderExecutionPort},
    position_use_cases::GetPositionsUseCase,
};
use domain::{EntityId, Symbol};
use infrastructure::{
    cache::RedisCache,
    config::AppConfig,
    database::{repositories::SqlxAccountRepository, DatabasePool},
    external::{
        ConsoleNotificationAdapter, HttpClient, MockMarketDataAdapter, MockOrderExecutionAdapter,
    },
    logging,
};

/// CLI arguments
#[derive(Parser, Debug)]
#[command(name = "trading-core")]
#[command(about = "Trading platform CLI")]
#[command(version)]
struct Cli {
    /// Configuration file path (optional, overrides default loading)
    #[arg(short, long)]
    config: Option<String>,

    /// Environment (development, production, test)
    #[arg(short, long, env = "APP_ENVIRONMENT")]
    environment: Option<String>,

    /// Subcommand to execute
    #[command(subcommand)]
    command: Commands,
}

/// Available commands
#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the trading server
    Server {
        /// Host to bind to
        #[arg(short, long, default_value = "0.0.0.0")]
        host: String,
        /// Port to listen on
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },
    /// List accounts
    Accounts,
    /// List positions
    Positions {
        /// Account ID to filter by
        #[arg(short, long)]
        account_id: Option<EntityId>,
    },
    /// Create a new order
    Order {
        /// Account ID
        #[arg(short, long)]
        account_id: EntityId,
        /// Symbol to trade
        #[arg(short, long)]
        symbol: String,
        /// Side (buy/sell)
        #[arg(short, long)]
        side: OrderSideArg,
        /// Quantity
        #[arg(short, long)]
        quantity: rust_decimal::Decimal,
        /// Order type (market/limit/stop)
        #[arg(short, long, default_value = "market")]
        order_type: OrderTypeArg,
    },
    /// Check system health
    Health,
    /// Initialize database
    Init,
    /// Configuration management commands
    Config {
        /// Validate configuration and exit
        #[arg(long)]
        validate: bool,
        /// Show current configuration (with secrets masked)
        #[arg(long)]
        show: bool,
        /// Output format (text, json)
        #[arg(short, long, default_value = "text")]
        format: ConfigOutputFormat,
    },
}

/// Output format for config command
#[derive(Debug, Clone, Copy)]
enum ConfigOutputFormat {
    Text,
    Json,
}

impl std::str::FromStr for ConfigOutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            _ => Err(format!("Invalid format: {}. Use 'text' or 'json'", s)),
        }
    }
}

/// Order side argument
#[derive(Debug, Clone, Copy)]
enum OrderSideArg {
    Buy,
    Sell,
}

impl std::str::FromStr for OrderSideArg {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "buy" => Ok(Self::Buy),
            "sell" => Ok(Self::Sell),
            _ => Err(format!("Invalid side: {}", s)),
        }
    }
}

impl From<OrderSideArg> for OrderSideDto {
    fn from(arg: OrderSideArg) -> Self {
        match arg {
            OrderSideArg::Buy => OrderSideDto::Buy,
            OrderSideArg::Sell => OrderSideDto::Sell,
        }
    }
}

/// Order type argument
#[derive(Debug, Clone, Copy)]
enum OrderTypeArg {
    Market,
    Limit,
    Stop,
}

impl std::str::FromStr for OrderTypeArg {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "market" => Ok(Self::Market),
            "limit" => Ok(Self::Limit),
            "stop" => Ok(Self::Stop),
            _ => Err(format!("Invalid order type: {}", s)),
        }
    }
}

impl From<OrderTypeArg> for OrderTypeDto {
    fn from(arg: OrderTypeArg) -> Self {
        match arg {
            OrderTypeArg::Market => OrderTypeDto::Market,
            OrderTypeArg::Limit => OrderTypeDto::Limit,
            OrderTypeArg::Stop => OrderTypeDto::Stop,
        }
    }
}

/// Application state/context
struct AppContext {
    /// Configuration
    #[allow(dead_code)]
    config: AppConfig,
    /// Notification port
    notification_port: Arc<dyn NotificationPort>,
    /// Market data port
    market_data_port: Arc<dyn MarketDataPort>,
    /// Order execution port
    order_execution_port: Arc<dyn OrderExecutionPort>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments
    let cli = Cli::parse();

    // Handle config command early (before logging init)
    if let Commands::Config {
        validate,
        show,
        format,
    } = &cli.command
    {
        return handle_config_command(cli.config, cli.environment, *validate, *show, *format);
    }

    // Load configuration
    let config = load_configuration(cli.config, cli.environment)?;

    // Initialize logging
    logging::init_tracing(&infrastructure::config::LoggingConfig {
        level: config.logging.level.clone(),
        format: config.logging.format.clone(),
        output: config.logging.output.clone(),
    })?;

    info!("Starting trading-core CLI");
    info!("Environment: {}", config.app.environment);
    info!("Version: {}", config.app.version);

    // Create application context with mock implementations
    let ctx = Arc::new(AppContext {
        config: config.clone(),
        notification_port: Arc::new(ConsoleNotificationAdapter::new()),
        market_data_port: Arc::new(MockMarketDataAdapter::new(HttpClient::new(
            "https://api.example.com",
            None,
        ))),
        order_execution_port: Arc::new(MockOrderExecutionAdapter::new(HttpClient::new(
            "https://api.example.com",
            None,
        ))),
    });

    // Execute command
    match cli.command {
        Commands::Server { host, port } => {
            run_server(&config, &host, port).await?;
        }
        Commands::Accounts => {
            list_accounts().await?;
        }
        Commands::Positions { account_id } => {
            list_positions(account_id).await?;
        }
        Commands::Order {
            account_id,
            symbol,
            side,
            quantity,
            order_type,
        } => {
            create_order(ctx, account_id, symbol, side, quantity, order_type).await?;
        }
        Commands::Health => {
            check_health(&config).await?;
        }
        Commands::Init => {
            init_database(&config).await?;
        }
        Commands::Config { .. } => {
            // Already handled above
        }
    }

    Ok(())
}

/// Loads configuration from file or environment
fn load_configuration(
    config_path: Option<String>,
    environment: Option<String>,
) -> Result<AppConfig> {
    let config = match (config_path, environment) {
        (Some(path), _) => AppConfig::from_file(path)?,
        (None, Some(env)) => AppConfig::load_with_environment(&env)?,
        (None, None) => AppConfig::load().unwrap_or_default(),
    };
    Ok(config)
}

/// Handles the config subcommand
fn handle_config_command(
    config_path: Option<String>,
    environment: Option<String>,
    validate: bool,
    show: bool,
    format: ConfigOutputFormat,
) -> Result<()> {
    // Load configuration without validation first if we're going to validate it
    let load_result = match (config_path.clone(), environment.clone()) {
        (Some(path), _) => AppConfig::from_file(path),
        (None, Some(env)) => AppConfig::load_with_environment(&env),
        (None, None) => AppConfig::load(),
    };

    match load_result {
        Ok(config) => {
            if validate {
                // Configuration is already validated by load()
                println!("✅ Configuration valid");
                Ok(())
            } else if show {
                display_config(&config, format)?;
                Ok(())
            } else {
                // Default action: show
                display_config(&config, format)?;
                Ok(())
            }
        }
        Err(e) => {
            if validate {
                println!("❌ Configuration error: {}", e);
                std::process::exit(1);
            } else {
                Err(e.into())
            }
        }
    }
}

/// Displays configuration in the specified format
fn display_config(config: &AppConfig, format: ConfigOutputFormat) -> Result<()> {
    let masked = config.masked();

    match format {
        ConfigOutputFormat::Text => {
            println!("{}", masked);
        }
        ConfigOutputFormat::Json => {
            let json = serde_json::to_string_pretty(&masked)?;
            println!("{}", json);
        }
    }

    Ok(())
}

/// Runs the trading server
async fn run_server(config: &AppConfig, host: &str, port: u16) -> Result<()> {
    info!("Starting server on {}:{}", host, port);
    info!("Database: {}", config.database.masked_url());
    info!("Redis: {}", config.redis.masked_url());

    // In a full implementation, this would start:
    // - HTTP API server
    // - WebSocket server for real-time updates
    // - Background workers
    // - Event consumers

    println!("🚀 Trading Core Server started on {}:{}", host, port);
    println!("📊 Database: {}", config.database.masked_url());
    println!("💾 Redis: {}", config.redis.masked_url());

    // Keep the server running
    tokio::signal::ctrl_c().await?;
    info!("Shutting down server...");

    Ok(())
}

/// Lists all accounts
async fn list_accounts() -> Result<()> {
    println!("📋 Listing accounts...");
    // In a full implementation, this would:
    // 1. Connect to database
    // 2. Fetch all accounts
    // 3. Display in a table format
    println!("No accounts found (mock implementation)");
    Ok(())
}

/// Lists positions
async fn list_positions(account_id: Option<EntityId>) -> Result<()> {
    match account_id {
        Some(id) => println!("📈 Listing positions for account {}...", id),
        None => println!("📈 Listing all positions..."),
    }
    // In a full implementation, this would fetch from database
    println!("No positions found (mock implementation)");
    Ok(())
}

/// Creates a new order
async fn create_order(
    ctx: Arc<AppContext>,
    account_id: EntityId,
    symbol: String,
    side: OrderSideArg,
    quantity: rust_decimal::Decimal,
    order_type: OrderTypeArg,
) -> Result<()> {
    info!(
        account_id = %account_id,
        symbol = %symbol,
        side = ?side,
        quantity = %quantity,
        "Creating order"
    );

    // Validate symbol
    let symbol = Symbol::new(&symbol)?;

    // Create order DTO
    let dto = CreateOrderDto {
        account_id,
        symbol: symbol.as_str().to_string(),
        side: side.into(),
        order_type: order_type.into(),
        quantity,
        limit_price: None,
        stop_price: None,
    };

    println!(
        "📝 Order created (mock): {:?} {} {} shares",
        dto.side, dto.symbol, dto.quantity
    );

    // Notify
    ctx.notification_port
        .notify(
            &format!(
                "Order created for {}: {:?} {}",
                dto.symbol, dto.side, dto.quantity
            ),
            NotificationLevel::Info,
        )
        .await?;

    Ok(())
}

/// Checks system health
async fn check_health(config: &AppConfig) -> Result<()> {
    println!("🏥 Checking system health...");

    // Check database connectivity
    print!("  Database... ");
    match DatabasePool::new(&config.database).await {
        Ok(_) => println!("✅ OK"),
        Err(e) => println!("❌ Failed: {}", e),
    }

    // Check Redis connectivity
    print!("  Redis... ");
    match RedisCache::new(&config.redis).await {
        Ok(_) => println!("✅ OK"),
        Err(e) => println!("❌ Failed: {}", e),
    }

    println!("✅ Health check complete");
    Ok(())
}

/// Initializes the database
async fn init_database(config: &AppConfig) -> Result<()> {
    println!("🗄️  Initializing database...");

    let pool = DatabasePool::new(&config.database).await?;
    pool.migrate().await?;

    println!("✅ Database initialized successfully");
    Ok(())
}
