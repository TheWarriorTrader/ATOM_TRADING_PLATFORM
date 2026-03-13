//! # Configuration Management
//!
//! Application configuration loading and management with validation.
//!
//! ## Configuration Loading Order (Precedence: Last wins)
//!
//! 1. `config/default.yaml` - Base configuration
//! 2. `config/{environment}.yaml` - Environment-specific overrides
//! 3. Environment variables with `APP_` prefix (e.g., `APP_DATABASE__HOST`)
//! 4. Command-line arguments (if provided)
//!
//! ## Environment Variables
//!
//! All config fields can be overridden via environment variables using the `APP_` prefix
//! and `__` as separator for nested fields:
//!
//! - `APP_APP__ENVIRONMENT=production`
//! - `APP_DATABASE__HOST=prod.db.com`
//! - `APP_DATABASE__PORT=5432`
//! - `APP_SERVER__PORT=8080`

use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::sync::RwLock;

use config::{Config, ConfigError, Environment, File};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError, ValidationErrors};

use crate::errors::{InfrastructureError, InfrastructureResult};

/// Global configuration instance for hot-reload support
static CONFIG_CACHE: Lazy<RwLock<Option<AppConfig>>> = Lazy::new(|| RwLock::new(None));

/// Validates that a port is within valid range (1-65535)
fn validate_port(port: u16) -> Result<(), ValidationError> {
    if port == 0 {
        return Err(ValidationError::new("port_zero"));
    }
    Ok(())
}

/// Validates that a string is not empty
fn validate_non_empty(s: &str) -> Result<(), ValidationError> {
    if s.trim().is_empty() {
        return Err(ValidationError::new("empty_string"));
    }
    Ok(())
}

/// Validates Redis database number (0-15)
fn validate_redis_db(db: i64) -> Result<(), ValidationError> {
    if db < 0 || db > 15 {
        return Err(ValidationError::new("invalid_redis_db"));
    }
    Ok(())
}

/// Validates log level
fn validate_log_level(level: &str) -> Result<(), ValidationError> {
    let valid_levels = ["trace", "debug", "info", "warn", "error"];
    if !valid_levels.contains(&level.to_lowercase().as_str()) {
        return Err(ValidationError::new("invalid_log_level"));
    }
    Ok(())
}

/// Validates log format
fn validate_log_format(format: &str) -> Result<(), ValidationError> {
    let valid_formats = ["json", "pretty", "compact"];
    if !valid_formats.contains(&format.to_lowercase().as_str()) {
        return Err(ValidationError::new("invalid_log_format"));
    }
    Ok(())
}

/// Validates exchange name
fn validate_exchange(exchange: &str) -> Result<(), ValidationError> {
    let valid_exchanges = ["IB", "SIMULATED", "BINANCE", "COINBASE"];
    if !valid_exchanges.contains(&exchange.to_uppercase().as_str()) {
        return Err(ValidationError::new("invalid_exchange"));
    }
    Ok(())
}

// =============================================================================
// App Configuration
// =============================================================================

/// Application metadata configuration
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct AppMetadataConfig {
    /// Application name
    #[validate(length(min = 1, message = "app.name cannot be empty"))]
    pub name: String,

    /// Application version
    #[validate(length(min = 1, message = "app.version cannot be empty"))]
    pub version: String,

    /// Environment (development, production, test)
    #[validate(custom(function = "validate_environment"))]
    pub environment: String,
}

fn validate_environment(env: &str) -> Result<(), ValidationError> {
    let valid = ["development", "production", "test"];
    if !valid.contains(&env.to_lowercase().as_str()) {
        return Err(ValidationError::new("invalid_environment"));
    }
    Ok(())
}

// =============================================================================
// Database Configuration
// =============================================================================

/// Database configuration with validation
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct DatabaseConfig {
    /// Database host
    #[validate(length(min = 1, message = "database.host cannot be empty"))]
    pub host: String,

    /// Database port (1-65535)
    #[validate(range(
        min = 1,
        max = 65535,
        message = "database.port must be between 1 and 65535"
    ))]
    pub port: u16,

    /// Database name
    #[validate(length(min = 1, message = "database.name cannot be empty"))]
    pub name: String,

    /// Database user
    #[validate(length(min = 1, message = "database.user cannot be empty"))]
    pub user: String,

    /// Database password
    #[validate(length(min = 1, message = "database.password cannot be empty"))]
    pub password: String,

    /// Maximum connections in pool
    #[validate(range(
        min = 1,
        max = 1000,
        message = "database.max_connections must be between 1 and 1000"
    ))]
    pub max_connections: u32,

    /// Connection timeout in milliseconds
    #[validate(range(
        min = 100,
        max = 60000,
        message = "database.timeout_ms must be between 100 and 60000"
    ))]
    pub timeout_ms: u64,
}

impl DatabaseConfig {
    /// Returns the database connection URL
    ///
    /// # Example
    ///
    /// ```
    /// # use infrastructure::config::DatabaseConfig;
    /// let config = DatabaseConfig {
    ///     host: "localhost".to_string(),
    ///     port: 5432,
    ///     name: "trading".to_string(),
    ///     user: "trader".to_string(),
    ///     password: "secret".to_string(),
    ///     max_connections: 10,
    ///     timeout_ms: 5000,
    /// };
    /// assert!(config.url().contains("postgres://"));
    /// ```
    #[must_use]
    pub fn url(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.user, self.password, self.host, self.port, self.name
        )
    }

    /// Returns connection URL with masked password for logging
    #[must_use]
    pub fn masked_url(&self) -> String {
        format!(
            "postgres://{}:***@{}:{}/{}",
            self.user, self.host, self.port, self.name
        )
    }
}

// =============================================================================
// Redis Configuration
// =============================================================================

/// Redis configuration with validation
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct RedisConfig {
    /// Redis host
    #[validate(length(min = 1, message = "redis.host cannot be empty"))]
    pub host: String,

    /// Redis port (1-65535)
    #[validate(range(
        min = 1,
        max = 65535,
        message = "redis.port must be between 1 and 65535"
    ))]
    pub port: u16,

    /// Redis password (optional)
    pub password: Option<String>,

    /// Redis database number (0-15)
    #[validate(range(min = 0, max = 15, message = "redis.db must be between 0 and 15"))]
    pub db: i64,

    /// Connection timeout in milliseconds
    #[validate(range(
        min = 100,
        max = 60000,
        message = "redis.timeout_ms must be between 100 and 60000"
    ))]
    pub timeout_ms: u64,
}

impl RedisConfig {
    /// Returns the Redis connection URL
    #[must_use]
    pub fn url(&self) -> String {
        match &self.password {
            Some(pwd) => format!("redis://:{}@{}:{}/{}", pwd, self.host, self.port, self.db),
            None => format!("redis://{}:{}/{}", self.host, self.port, self.db),
        }
    }

    /// Returns connection URL with masked password for logging
    #[must_use]
    pub fn masked_url(&self) -> String {
        match &self.password {
            Some(_) => format!("redis://:***@{}:{}/{}", self.host, self.port, self.db),
            None => format!("redis://{}:{}/{}", self.host, self.port, self.db),
        }
    }
}

// =============================================================================
// Logging Configuration
// =============================================================================

/// Logging configuration with validation
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    #[validate(custom(function = "validate_log_level"))]
    pub level: String,

    /// Log format (json, pretty, compact)
    #[validate(custom(function = "validate_log_format"))]
    pub format: String,

    /// Output destination (stdout, stderr, or file path)
    #[validate(length(min = 1, message = "logging.output cannot be empty"))]
    pub output: String,
}

// =============================================================================
// Trading Configuration
// =============================================================================

/// Trading configuration with validation
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct TradingConfig {
    /// Enable risk management
    pub risk_enabled: bool,

    /// Maximum position size in lots/contracts
    #[validate(range(
        min = 1,
        max = 10000,
        message = "trading.max_position_size must be between 1 and 10000"
    ))]
    pub max_position_size: u32,

    /// Default exchange (IB, SIMULATED, BINANCE, COINBASE)
    #[validate(custom(function = "validate_exchange"))]
    pub default_exchange: String,

    /// Paper trading mode
    pub paper_trading: bool,
}

// =============================================================================
// Server Configuration
// =============================================================================

/// Server configuration with validation
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct ServerConfig {
    /// Server host
    #[validate(length(min = 1, message = "server.host cannot be empty"))]
    pub host: String,

    /// Server port (1-65535)
    #[validate(range(
        min = 1,
        max = 65535,
        message = "server.port must be between 1 and 65535"
    ))]
    pub port: u16,

    /// Request timeout in milliseconds
    #[validate(range(
        min = 1000,
        max = 300000,
        message = "server.request_timeout_ms must be between 1000 and 300000"
    ))]
    pub request_timeout_ms: u64,
}

// =============================================================================
// Main App Config
// =============================================================================

/// Application configuration with validation
///
/// This is the main configuration structure that holds all application settings.
/// It is loaded from YAML files and environment variables with validation.
///
/// # Example
///
/// ```rust
/// use infrastructure::config::AppConfig;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let config = AppConfig::load()?;
/// println!("Database URL: {}", config.database.masked_url());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct AppConfig {
    /// Application metadata
    pub app: AppMetadataConfig,

    /// Database configuration
    pub database: DatabaseConfig,

    /// Redis configuration
    pub redis: RedisConfig,

    /// Logging configuration
    pub logging: LoggingConfig,

    /// Trading configuration
    pub trading: TradingConfig,

    /// Server configuration
    pub server: ServerConfig,
}

impl AppConfig {
    /// Loads configuration from files and environment variables
    ///
    /// Configuration loading order (later overrides earlier):
    /// 1. `config/default.yaml`
    /// 2. `config/{environment}.yaml` where environment is from APP_ENVIRONMENT or "development"
    /// 3. Environment variables with `APP_` prefix
    ///
    /// # Errors
    ///
    /// Returns `InfrastructureError::Configuration` if:
    /// - Configuration files cannot be read
    /// - Configuration is invalid YAML
    /// - Validation fails
    ///
    /// # Example
    ///
    /// ```rust
    /// use infrastructure::config::AppConfig;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = AppConfig::load()?;
    /// println!("Loaded config for environment: {}", config.app.environment);
    /// # Ok(())
    /// # }
    /// ```
    pub fn load() -> InfrastructureResult<Self> {
        let environment = std::env::var("APP_ENVIRONMENT")
            .or_else(|_| std::env::var("RUN_MODE"))
            .unwrap_or_else(|_| "development".to_string());

        let config = Self::load_with_environment(&environment)?;

        // Validate the configuration
        config.validate().map_err(|e| {
            InfrastructureError::configuration(format!(
                "Validation failed: {}",
                Self::format_validation_errors(&e)
            ))
        })?;

        // Cache the configuration
        if let Ok(mut cache) = CONFIG_CACHE.write() {
            *cache = Some(config.clone());
        }

        Ok(config)
    }

    /// Loads configuration with a specific environment
    ///
    /// # Arguments
    ///
    /// * `environment` - The environment name (e.g., "development", "production", "test")
    ///
    /// # Errors
    ///
    /// Returns `InfrastructureError::Configuration` if loading fails
    pub fn load_with_environment(environment: &str) -> InfrastructureResult<Self> {
        let builder = Config::builder()
            // 1. Load default configuration
            .add_source(File::with_name("config/default").required(false))
            // 2. Load environment-specific configuration
            .add_source(File::with_name(&format!("config/{}", environment)).required(false))
            // 3. Load local configuration (gitignored, for local overrides)
            .add_source(File::with_name("config/local").required(false))
            // 4. Load environment variables with APP_ prefix
            .add_source(
                Environment::with_prefix("APP")
                    .separator("__")
                    .try_parsing(true),
            );

        let config = builder.build().map_err(|e| {
            InfrastructureError::configuration(format!("Failed to build config: {}", e))
        })?;

        config.try_deserialize::<Self>().map_err(|e| {
            InfrastructureError::configuration(format!("Failed to deserialize config: {}", e))
        })
    }

    /// Loads configuration from a specific file path
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the configuration file
    ///
    /// # Errors
    ///
    /// Returns `InfrastructureError::Configuration` if file cannot be read or parsed
    ///
    /// # Example
    ///
    /// ```rust
    /// use infrastructure::config::AppConfig;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # #[cfg(feature = "test-utils")]
    /// let config = AppConfig::from_file("config/custom.yaml")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_file<P: AsRef<Path>>(path: P) -> InfrastructureResult<Self> {
        let config = Config::builder()
            .add_source(File::from(path.as_ref()))
            .build()
            .map_err(|e| {
                InfrastructureError::configuration(format!("Failed to load file: {}", e))
            })?;

        let app_config: Self = config.try_deserialize().map_err(|e| {
            InfrastructureError::configuration(format!("Failed to parse config: {}", e))
        })?;

        // Validate the configuration
        app_config.validate().map_err(|e| {
            InfrastructureError::configuration(format!(
                "Validation failed: {}",
                Self::format_validation_errors(&e)
            ))
        })?;

        Ok(app_config)
    }

    /// Validates the configuration and returns detailed error messages
    ///
    /// # Returns
    ///
    /// * `Ok(())` if configuration is valid
    /// * `Err(String)` with detailed error message if invalid
    pub fn validate_and_report(&self) -> Result<(), String> {
        match self.validate() {
            Ok(()) => Ok(()),
            Err(e) => Err(Self::format_validation_errors(&e)),
        }
    }

    /// Returns a masked version of the configuration for safe display
    ///
    /// Passwords and secrets are replaced with `***`
    #[must_use]
    pub fn masked(&self) -> MaskedAppConfig {
        MaskedAppConfig::from(self)
    }

    /// Formats validation errors into a human-readable string
    fn format_validation_errors(errors: &ValidationErrors) -> String {
        let mut messages = Vec::new();

        for (field, field_errors) in errors.field_errors() {
            for error in field_errors {
                let msg = error
                    .message
                    .as_ref()
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| format!("Invalid value for {}", field));
                messages.push(msg);
            }
        }

        // Handle struct-level errors (skipped due to API changes in validator)
        // Note: struct_errors() is not available in this version of validator

        messages.join("; ")
    }

    /// Returns the cached configuration if available
    ///
    /// This is useful for accessing configuration without reloading
    pub fn cached() -> Option<Self> {
        CONFIG_CACHE.read().ok().and_then(|cache| cache.clone())
    }

    /// Clears the configuration cache
    pub fn clear_cache() {
        if let Ok(mut cache) = CONFIG_CACHE.write() {
            *cache = None;
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            app: AppMetadataConfig {
                name: "trading-core".to_string(),
                version: "0.1.0".to_string(),
                environment: "development".to_string(),
            },
            database: DatabaseConfig {
                host: "localhost".to_string(),
                port: 5432,
                name: "trading_db".to_string(),
                user: "trader".to_string(),
                password: "trader".to_string(),
                max_connections: 10,
                timeout_ms: 5000,
            },
            redis: RedisConfig {
                host: "localhost".to_string(),
                port: 6379,
                password: None,
                db: 0,
                timeout_ms: 2000,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: "json".to_string(),
                output: "stdout".to_string(),
            },
            trading: TradingConfig {
                risk_enabled: true,
                max_position_size: 100,
                default_exchange: "IB".to_string(),
                paper_trading: true,
            },
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 8080,
                request_timeout_ms: 30000,
            },
        }
    }
}

// =============================================================================
// Masked Configuration for Safe Display
// =============================================================================

/// A masked version of AppConfig for safe display/logging
///
/// All sensitive fields (passwords, secrets) are replaced with `***`
#[derive(Debug, Clone, Serialize)]
pub struct MaskedAppConfig {
    pub app: MaskedAppMetadataConfig,
    pub database: MaskedDatabaseConfig,
    pub redis: MaskedRedisConfig,
    pub logging: LoggingConfig,
    pub trading: TradingConfig,
    pub server: ServerConfig,
}

#[derive(Debug, Clone, Serialize)]
pub struct MaskedAppMetadataConfig {
    pub name: String,
    pub version: String,
    pub environment: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MaskedDatabaseConfig {
    pub host: String,
    pub port: u16,
    pub name: String,
    pub user: String,
    pub password: String, // masked
    pub max_connections: u32,
    pub timeout_ms: u64,
    pub url: String, // masked URL
}

#[derive(Debug, Clone, Serialize)]
pub struct MaskedRedisConfig {
    pub host: String,
    pub port: u16,
    pub password: Option<String>, // masked
    pub db: i64,
    pub timeout_ms: u64,
    pub url: String, // masked URL
}

impl From<&AppConfig> for MaskedAppConfig {
    fn from(config: &AppConfig) -> Self {
        Self {
            app: MaskedAppMetadataConfig {
                name: config.app.name.clone(),
                version: config.app.version.clone(),
                environment: config.app.environment.clone(),
            },
            database: MaskedDatabaseConfig {
                host: config.database.host.clone(),
                port: config.database.port,
                name: config.database.name.clone(),
                user: config.database.user.clone(),
                password: "***".to_string(),
                max_connections: config.database.max_connections,
                timeout_ms: config.database.timeout_ms,
                url: config.database.masked_url(),
            },
            redis: MaskedRedisConfig {
                host: config.redis.host.clone(),
                port: config.redis.port,
                password: config.redis.password.as_ref().map(|_| "***".to_string()),
                db: config.redis.db,
                timeout_ms: config.redis.timeout_ms,
                url: config.redis.masked_url(),
            },
            logging: config.logging.clone(),
            trading: config.trading.clone(),
            server: config.server.clone(),
        }
    }
}

impl fmt::Display for MaskedAppConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "📋 Configuration:")?;
        writeln!(f, "  App:")?;
        writeln!(f, "    Name:        {}", self.app.name)?;
        writeln!(f, "    Version:     {}", self.app.version)?;
        writeln!(f, "    Environment: {}", self.app.environment)?;
        writeln!(f, "  Database:")?;
        writeln!(f, "    Host:        {}", self.database.host)?;
        writeln!(f, "    Port:        {}", self.database.port)?;
        writeln!(f, "    Name:        {}", self.database.name)?;
        writeln!(f, "    User:        {}", self.database.user)?;
        writeln!(f, "    Password:    {}", self.database.password)?;
        writeln!(f, "    Pool Size:   {}", self.database.max_connections)?;
        writeln!(f, "    Timeout:     {}ms", self.database.timeout_ms)?;
        writeln!(f, "    URL:         {}", self.database.url)?;
        writeln!(f, "  Redis:")?;
        writeln!(f, "    Host:        {}", self.redis.host)?;
        writeln!(f, "    Port:        {}", self.redis.port)?;
        writeln!(f, "    Password:    {:?}", self.redis.password)?;
        writeln!(f, "    DB:          {}", self.redis.db)?;
        writeln!(f, "    Timeout:     {}ms", self.redis.timeout_ms)?;
        writeln!(f, "    URL:         {}", self.redis.url)?;
        writeln!(f, "  Logging:")?;
        writeln!(f, "    Level:       {}", self.logging.level)?;
        writeln!(f, "    Format:      {}", self.logging.format)?;
        writeln!(f, "    Output:      {}", self.logging.output)?;
        writeln!(f, "  Trading:")?;
        writeln!(f, "    Risk:        {}", self.trading.risk_enabled)?;
        writeln!(f, "    Max Pos:     {}", self.trading.max_position_size)?;
        writeln!(f, "    Exchange:    {}", self.trading.default_exchange)?;
        writeln!(f, "    Paper:       {}", self.trading.paper_trading)?;
        writeln!(f, "  Server:")?;
        writeln!(f, "    Host:        {}", self.server.host)?;
        writeln!(f, "    Port:        {}", self.server.port)?;
        writeln!(f, "    Timeout:     {}ms", self.server.request_timeout_ms)
    }
}

// =============================================================================
// Legacy API Compatibility
// =============================================================================

/// API configuration for backward compatibility
#[derive(Debug, Clone, Deserialize)]
pub struct ApiConfig {
    /// API host
    pub host: String,
    /// API port
    pub port: u16,
    /// API key for external services
    pub api_key: Option<String>,
}

// Re-export for backward compatibility
pub use crate::config::DatabaseConfig as LegacyDatabaseConfig;
pub use crate::config::RedisConfig as LegacyRedisConfig;
