//! # Value Objects
//!
//! Immutable value objects for financial calculations.
//! All values are validated at construction time.
//!
//! ## Design Principles
//!
//! - **Type-safe**: No primitive obsession (never use `f64` or `String` directly)
//! - **Fail-fast validation**: Construction fails if input is invalid
//! - **Immutability**: All fields are `pub` but structs themselves are immutable
//! - **Zero-cost abstractions**: Newtype pattern with no runtime overhead

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

#[allow(unused_imports)]
use crate::errors::{DomainError, DomainResult};

// =============================================================================
// ERROR TYPES
// =============================================================================

/// Errors that can occur when working with value objects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueError {
    /// Invalid price value
    InvalidPrice(String),
    /// Invalid quantity value
    InvalidQuantity(String),
    /// Invalid symbol format
    InvalidSymbol(String),
    /// Invalid volume value
    InvalidVolume(String),
    /// Invalid order ID
    InvalidOrderId(String),
    /// Invalid money amount
    InvalidMoney(String),
    /// Invalid timeframe
    InvalidTimeFrame(String),
    /// Invalid side
    InvalidSide(String),
    /// Invalid order type
    InvalidOrderType(String),
    /// Invalid time in force
    InvalidTimeInForce(String),
    /// Invalid currency
    InvalidCurrency(String),
}

impl fmt::Display for ValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPrice(msg) => write!(f, "Invalid price: {msg}"),
            Self::InvalidQuantity(msg) => write!(f, "Invalid quantity: {msg}"),
            Self::InvalidSymbol(msg) => write!(f, "Invalid symbol: {msg}"),
            Self::InvalidVolume(msg) => write!(f, "Invalid volume: {msg}"),
            Self::InvalidOrderId(msg) => write!(f, "Invalid order ID: {msg}"),
            Self::InvalidMoney(msg) => write!(f, "Invalid money: {msg}"),
            Self::InvalidTimeFrame(msg) => write!(f, "Invalid timeframe: {msg}"),
            Self::InvalidSide(msg) => write!(f, "Invalid side: {msg}"),
            Self::InvalidOrderType(msg) => write!(f, "Invalid order type: {msg}"),
            Self::InvalidTimeInForce(msg) => write!(f, "Invalid time in force: {msg}"),
            Self::InvalidCurrency(msg) => write!(f, "Invalid currency: {msg}"),
        }
    }
}

impl std::error::Error for ValueError {}

// =============================================================================
// PRICE
// =============================================================================

/// A validated price value.
///
/// Price wraps a [`Decimal`] and ensures the value is strictly positive.
/// Use this type instead of raw `Decimal` or `f64` to prevent
/// negative or zero prices in financial calculations.
///
/// # Examples
///
/// ```
/// use rust_decimal::Decimal;
/// use domain::values::Price;
///
/// let price = Price::new(Decimal::new(10050, 2)).unwrap(); // $100.50
/// assert_eq!(price.to_string(), "100.50");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Price(Decimal);

impl Price {
    /// Creates a new Price from a Decimal.
    ///
    /// # Errors
    ///
    /// Returns [`ValueError::InvalidPrice`] if the value is not positive.
    ///
    /// # Examples
    ///
    /// ```
    /// use rust_decimal::Decimal;
    /// use domain::values::Price;
    ///
    /// let price = Price::new(Decimal::new(10050, 2)).unwrap();
    /// assert_eq!(price.inner(), Decimal::new(10050, 2));
    /// ```
    pub fn new(value: Decimal) -> Result<Self, ValueError> {
        if value <= Decimal::ZERO {
            return Err(ValueError::InvalidPrice(format!(
                "Price must be positive, got {}",
                value
            )));
        }
        Ok(Self(value))
    }

    /// Creates a new Price from a Decimal without validation.
    ///
    /// # Safety
    ///
    /// Caller must ensure the value is positive. Using invalid values
    /// may lead to incorrect financial calculations.
    ///
    /// # Examples
    ///
    /// ```
    /// use rust_decimal::Decimal;
    /// use domain::values::Price;
    ///
    /// let price = unsafe { Price::new_unchecked(Decimal::new(100, 0)) };
    /// assert_eq!(price.inner(), Decimal::new(100, 0));
    /// ```
    ///
    /// # Safety
    ///
    /// This function is marked unsafe because creating a Price with
    /// a non-positive value violates the type's invariants.
    pub unsafe fn new_unchecked(value: Decimal) -> Self {
        Self(value)
    }

    /// Returns the underlying Decimal value.
    #[must_use]
    pub const fn inner(&self) -> Decimal {
        self.0
    }

    /// Returns a zero price.
    ///
    /// # Warning
    ///
    /// This creates an invalid Price (zero). Use with caution and
    /// only in contexts where zero prices are semantically valid.
    #[must_use]
    pub const fn zero() -> Self {
        Self(Decimal::ZERO)
    }

    /// Returns true if the price is strictly positive.
    #[must_use]
    pub fn is_positive(&self) -> bool {
        self.0 > Decimal::ZERO
    }

    /// Returns true if the price is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.0 == Decimal::ZERO
    }

    /// Multiplies the price by a scalar.
    ///
    /// # Examples
    ///
    /// ```
    /// use rust_decimal::Decimal;
    /// use domain::values::Price;
    ///
    /// let price = Price::new(Decimal::new(100, 0)).unwrap();
    /// let doubled = price * Decimal::new(2, 0);
    /// assert_eq!(doubled.inner(), Decimal::new(200, 0));
    /// ```
    #[must_use]
    pub fn mul(&self, scalar: Decimal) -> Self {
        Self(self.0 * scalar)
    }
}

impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Default to 2 decimal places for currency display
        write!(f, "{:.2}", self.0)
    }
}

impl AsRef<Decimal> for Price {
    fn as_ref(&self) -> &Decimal {
        &self.0
    }
}

impl FromStr for Price {
    type Err = ValueError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = Decimal::from_str(s)
            .map_err(|e| ValueError::InvalidPrice(format!("Failed to parse '{}': {}", s, e)))?;
        Self::new(value)
    }
}

impl Default for Price {
    fn default() -> Self {
        Self(Decimal::ZERO)
    }
}

// Allow multiplication by Decimal
impl std::ops::Mul<Decimal> for Price {
    type Output = Self;

    fn mul(self, rhs: Decimal) -> Self::Output {
        Self(self.0 * rhs)
    }
}

// =============================================================================
// SYMBOL
// =============================================================================

/// Maximum length for a symbol string.
const MAX_SYMBOL_LENGTH: usize = 20;

/// A validated trading symbol identifier.
///
/// Symbol wraps a [`String`] and ensures:
/// - Non-empty and not longer than 20 characters
/// - Contains only valid characters: alphanumeric, `/`, `-`, `.`, `:`
/// - Automatically normalized to uppercase
///
/// # Examples
///
/// ```
/// use domain::values::Symbol;
///
/// let symbol = Symbol::new("aapl").unwrap();
/// assert_eq!(symbol.as_str(), "AAPL");
///
/// let futures = Symbol::new("ES/2024").unwrap();
/// assert!(futures.is_futures());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Symbol(String);

impl Symbol {
    /// Creates a new Symbol from a string.
    ///
    /// # Validation
    ///
    /// - Must not be empty
    /// - Must not exceed 20 characters
    /// - Can contain: A-Z, 0-9, `/`, `-`, `.`, `:`
    /// - Automatically converted to uppercase
    ///
    /// # Errors
    ///
    /// Returns [`ValueError::InvalidSymbol`] if validation fails.
    pub fn new(symbol: impl Into<String>) -> Result<Self, ValueError> {
        let s = symbol.into();

        if s.is_empty() {
            return Err(ValueError::InvalidSymbol(
                "Symbol cannot be empty".to_string(),
            ));
        }

        if s.len() > MAX_SYMBOL_LENGTH {
            return Err(ValueError::InvalidSymbol(format!(
                "Symbol '{}' exceeds maximum length of {} characters",
                s, MAX_SYMBOL_LENGTH
            )));
        }

        // Validate characters: alphanumeric and some special chars for futures/options
        if !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '/' || c == '-' || c == '.' || c == ':')
        {
            return Err(ValueError::InvalidSymbol(format!(
                "Symbol '{}' contains invalid characters",
                s
            )));
        }

        Ok(Self(s.to_uppercase()))
    }

    /// Creates a new Symbol without validation.
    ///
    /// # Safety
    ///
    /// Caller must ensure the symbol is valid. Invalid symbols may
    /// cause issues with external APIs or databases.
    pub unsafe fn new_unchecked(symbol: impl Into<String>) -> Self {
        Self(symbol.into().to_uppercase())
    }

    /// Returns the symbol string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns true if this is a futures symbol (contains '/').
    #[must_use]
    pub fn is_futures(&self) -> bool {
        self.0.contains('/')
    }

    /// Returns true if this is an options symbol (contains ':').
    #[must_use]
    pub fn is_option(&self) -> bool {
        self.0.contains(':')
    }

    /// Returns the length of the symbol.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if the symbol is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for Symbol {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl FromStr for Symbol {
    type Err = ValueError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

// =============================================================================
// QUANTITY
// =============================================================================

/// A validated quantity value.
///
/// Quantity wraps a [`Decimal`] and ensures the value is strictly positive.
/// Supports both fractional quantities (for stocks) and integer quantities
/// (for futures).
///
/// # Examples
///
/// ```
/// use rust_decimal::Decimal;
/// use domain::values::Quantity;
///
/// let qty = Quantity::new(Decimal::new(1005, 2)).unwrap(); // 10.05 shares
/// assert!(!qty.is_integer());
///
/// let whole = Quantity::new(Decimal::new(100, 0)).unwrap();
/// assert!(whole.is_integer());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Quantity(Decimal);

impl Quantity {
    /// Creates a new Quantity from a Decimal.
    ///
    /// # Errors
    ///
    /// Returns [`ValueError::InvalidQuantity`] if the value is not positive.
    pub fn new(value: Decimal) -> Result<Self, ValueError> {
        if value <= Decimal::ZERO {
            return Err(ValueError::InvalidQuantity(format!(
                "Quantity must be positive, got {}",
                value
            )));
        }
        Ok(Self(value))
    }

    /// Creates a new Quantity without validation.
    ///
    /// # Safety
    ///
    /// Caller must ensure the value is positive.
    pub unsafe fn new_unchecked(value: Decimal) -> Self {
        Self(value)
    }

    /// Returns the underlying Decimal value.
    #[must_use]
    pub const fn inner(&self) -> Decimal {
        self.0
    }

    /// Returns true if the quantity is an integer (no fractional part).
    #[must_use]
    pub fn is_integer(&self) -> bool {
        self.0.fract() == Decimal::ZERO
    }

    /// Returns the floor (largest integer less than or equal to quantity).
    #[must_use]
    pub fn floor(&self) -> Self {
        Self(self.0.floor())
    }

    /// Returns the ceiling (smallest integer greater than or equal to quantity).
    #[must_use]
    pub fn ceil(&self) -> Self {
        Self(self.0.ceil())
    }

    /// Returns true if the quantity is strictly positive.
    #[must_use]
    pub fn is_positive(&self) -> bool {
        self.0 > Decimal::ZERO
    }

    /// Adds two quantities together.
    #[must_use]
    pub fn add(&self, other: &Self) -> Self {
        Self(self.0 + other.0)
    }

    /// Subtracts another quantity from this one.
    ///
    /// # Note
    ///
    /// This can result in a negative quantity. The result is not validated.
    #[must_use]
    pub fn sub(&self, other: &Self) -> Decimal {
        self.0 - other.0
    }
}

impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.normalize())
    }
}

impl AsRef<Decimal> for Quantity {
    fn as_ref(&self) -> &Decimal {
        &self.0
    }
}

impl FromStr for Quantity {
    type Err = ValueError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = Decimal::from_str(s)
            .map_err(|e| ValueError::InvalidQuantity(format!("Failed to parse '{}': {}", s, e)))?;
        Self::new(value)
    }
}

impl Default for Quantity {
    fn default() -> Self {
        Self(Decimal::ZERO)
    }
}

// =============================================================================
// VOLUME
// =============================================================================

/// A validated volume value (non-negative i64).
///
/// Volume represents the number of shares/contracts traded.
/// Unlike Quantity, Volume can be zero and is always an integer.
///
/// # Examples
///
/// ```
/// use domain::values::Volume;
///
/// let vol = Volume::new(1_000_000).unwrap();
/// assert_eq!(vol.as_i64(), 1_000_000);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Volume(i64);

impl Volume {
    /// Creates a new Volume from an i64.
    ///
    /// # Errors
    ///
    /// Returns [`ValueError::InvalidVolume`] if the value is negative.
    pub fn new(value: i64) -> Result<Self, ValueError> {
        if value < 0 {
            return Err(ValueError::InvalidVolume(format!(
                "Volume cannot be negative, got {}",
                value
            )));
        }
        Ok(Self(value))
    }

    /// Creates a new Volume without validation.
    ///
    /// # Safety
    ///
    /// Caller must ensure the value is non-negative.
    pub const unsafe fn new_unchecked(value: i64) -> Self {
        Self(value)
    }

    /// Returns the underlying i64 value.
    #[must_use]
    pub const fn as_i64(&self) -> i64 {
        self.0
    }

    /// Returns a zero volume.
    #[must_use]
    pub const fn zero() -> Self {
        Self(0)
    }

    /// Increments the volume by one.
    ///
    /// # Panics
    ///
    /// Panics if the volume would overflow i64.
    #[must_use]
    pub fn increment(&self) -> Self {
        Self(self.0.checked_add(1).expect("Volume overflow"))
    }

    /// Adds another volume to this one.
    ///
    /// # Panics
    ///
    /// Panics if the result would overflow i64.
    #[must_use]
    pub fn add(&self, other: &Self) -> Self {
        Self(
            self.0
                .checked_add(other.0)
                .expect("Volume addition overflow"),
        )
    }

    /// Returns true if the volume is zero.
    #[must_use]
    pub const fn is_zero(&self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for Volume {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<i64> for Volume {
    fn as_ref(&self) -> &i64 {
        &self.0
    }
}

impl FromStr for Volume {
    type Err = ValueError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = s
            .parse::<i64>()
            .map_err(|e| ValueError::InvalidVolume(format!("Failed to parse '{}': {}", s, e)))?;
        Self::new(value)
    }
}

impl Default for Volume {
    fn default() -> Self {
        Self(0)
    }
}

// =============================================================================
// ORDER ID
// =============================================================================

/// A unique order identifier.
///
/// OrderId wraps a UUID and provides automatic generation.
/// Display format includes an "ord-" prefix for readability.
///
/// # Examples
///
/// ```
/// use domain::values::OrderId;
///
/// let id = OrderId::generate();
/// assert!(id.to_string().starts_with("ord-"));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrderId(Uuid);

impl OrderId {
    /// Generates a new random OrderId using UUID v4.
    #[must_use]
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    /// Creates an OrderId from an existing UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Creates an OrderId from a string representation.
    ///
    /// # Errors
    ///
    /// Returns [`ValueError::InvalidOrderId`] if the string is not a valid UUID.
    pub fn from_str(s: &str) -> Result<Self, ValueError> {
        // Strip prefix if present
        let uuid_str = if s.starts_with("ord-") { &s[4..] } else { s };

        let uuid = Uuid::parse_str(uuid_str)
            .map_err(|e| ValueError::InvalidOrderId(format!("Failed to parse '{}': {}", s, e)))?;
        Ok(Self(uuid))
    }

    /// Returns the underlying UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }

    /// Returns the string representation without prefix.
    #[must_use]
    pub fn to_simple_string(&self) -> String {
        self.0.to_string()
    }
}

impl fmt::Display for OrderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ord-{}", self.0)
    }
}

impl AsRef<Uuid> for OrderId {
    fn as_ref(&self) -> &Uuid {
        &self.0
    }
}

impl Default for OrderId {
    fn default() -> Self {
        Self::generate()
    }
}

// =============================================================================
// CURRENCY
// =============================================================================

/// ISO 4217 currency codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Currency {
    /// United States Dollar
    USD,
    /// Euro
    EUR,
    /// British Pound Sterling
    GBP,
    /// Japanese Yen
    JPY,
    /// Swiss Franc
    CHF,
    /// Canadian Dollar
    CAD,
    /// Australian Dollar
    AUD,
    /// Chinese Yuan
    CNY,
}

impl Currency {
    /// Returns the 3-letter ISO 4217 code.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::USD => "USD",
            Self::EUR => "EUR",
            Self::GBP => "GBP",
            Self::JPY => "JPY",
            Self::CHF => "CHF",
            Self::CAD => "CAD",
            Self::AUD => "AUD",
            Self::CNY => "CNY",
        }
    }

    /// Returns the number of decimal places for this currency.
    ///
    /// Most currencies use 2 decimal places, but some like JPY use 0.
    #[must_use]
    pub const fn decimal_places(&self) -> u32 {
        match self {
            Self::JPY => 0,
            _ => 2,
        }
    }

    /// Returns all supported currencies.
    #[must_use]
    pub fn all() -> &'static [Currency] {
        &[
            Self::USD,
            Self::EUR,
            Self::GBP,
            Self::JPY,
            Self::CHF,
            Self::CAD,
            Self::AUD,
            Self::CNY,
        ]
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for Currency {
    type Err = ValueError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "USD" => Ok(Self::USD),
            "EUR" => Ok(Self::EUR),
            "GBP" => Ok(Self::GBP),
            "JPY" => Ok(Self::JPY),
            "CHF" => Ok(Self::CHF),
            "CAD" => Ok(Self::CAD),
            "AUD" => Ok(Self::AUD),
            "CNY" => Ok(Self::CNY),
            _ => Err(ValueError::InvalidCurrency(format!(
                "Unknown currency code: {}",
                s
            ))),
        }
    }
}

impl Default for Currency {
    fn default() -> Self {
        Self::USD
    }
}

// =============================================================================
// MONEY
// =============================================================================

/// A monetary amount with currency.
///
/// Money combines an amount ([`Decimal`]) with a [`Currency`].
/// The amount can be zero or positive (negative amounts for debits
/// should use separate types or explicit signs).
///
/// # Examples
///
/// ```
/// use rust_decimal::Decimal;
/// use domain::values::{Money, Currency};
///
/// let money = Money::new(Decimal::new(10050, 2), Currency::USD).unwrap();
/// assert_eq!(money.to_string(), "100.50 USD");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Money {
    /// The monetary amount
    amount: Decimal,
    /// The currency code (ISO 4217)
    currency: Currency,
}

impl Money {
    /// Creates a new Money instance.
    ///
    /// # Arguments
    ///
    /// * `amount` - The monetary amount (can be zero or positive)
    /// * `currency` - The currency code
    ///
    /// # Errors
    ///
    /// Returns [`ValueError::InvalidMoney`] if the amount is negative.
    pub fn new(amount: Decimal, currency: Currency) -> Result<Self, ValueError> {
        if amount < Decimal::ZERO {
            return Err(ValueError::InvalidMoney(
                "Money amount cannot be negative".to_string(),
            ));
        }
        Ok(Self { amount, currency })
    }

    /// Creates a new Money instance without validation.
    ///
    /// # Safety
    ///
    /// Caller must ensure the amount is non-negative.
    pub unsafe fn new_unchecked(amount: Decimal, currency: Currency) -> Self {
        Self { amount, currency }
    }

    /// Returns the amount.
    #[must_use]
    pub const fn amount(&self) -> Decimal {
        self.amount
    }

    /// Returns the currency.
    #[must_use]
    pub const fn currency(&self) -> Currency {
        self.currency
    }

    /// Returns a formatted string with the currency symbol.
    ///
    /// # Examples
    ///
    /// ```
    /// use rust_decimal::Decimal;
    /// use domain::values::{Money, Currency};
    ///
    /// let money = Money::new(Decimal::new(10050, 2), Currency::USD).unwrap();
    /// assert_eq!(money.format(), "$100.50");
    /// ```
    #[must_use]
    pub fn format(&self) -> String {
        let symbol = match self.currency {
            Currency::USD => "$",
            Currency::EUR => "€",
            Currency::GBP => "£",
            Currency::JPY => "¥",
            Currency::CHF => "CHF ",
            Currency::CAD => "C$",
            Currency::AUD => "A$",
            Currency::CNY => "¥",
        };
        format!("{}{:.2}", symbol, self.amount)
    }

    /// Adds two Money values together.
    ///
    /// # Errors
    ///
    /// Returns [`ValueError::InvalidMoney`] if currencies don't match.
    pub fn add(&self, other: &Self) -> Result<Self, ValueError> {
        if self.currency != other.currency {
            return Err(ValueError::InvalidMoney(
                "Cannot add Money with different currencies".to_string(),
            ));
        }
        Ok(Self {
            amount: self.amount + other.amount,
            currency: self.currency,
        })
    }

    /// Multiplies the money amount by a scalar.
    #[must_use]
    pub fn mul(&self, scalar: Decimal) -> Self {
        Self {
            amount: self.amount * scalar,
            currency: self.currency,
        }
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.amount, self.currency)
    }
}

impl Default for Money {
    fn default() -> Self {
        Self {
            amount: Decimal::ZERO,
            currency: Currency::default(),
        }
    }
}

// =============================================================================
// TIMEFRAME
// =============================================================================

/// Trading timeframe intervals.
///
/// TimeFrame represents the duration of a single candle/bar in
/// technical analysis. Commonly used values from 1 minute to 1 week.
///
/// # Examples
///
/// ```
/// use domain::values::TimeFrame;
/// use std::time::Duration;
///
/// let tf = TimeFrame::H1;
/// assert_eq!(tf.duration(), Duration::from_secs(3600));
/// assert_eq!(tf.as_str(), "1h");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TimeFrame {
    /// 1 minute
    M1,
    /// 5 minutes
    M5,
    /// 15 minutes
    M15,
    /// 30 minutes
    M30,
    /// 1 hour
    H1,
    /// 4 hours
    H4,
    /// 1 day
    D1,
    /// 1 week
    W1,
}

impl TimeFrame {
    /// Returns the duration in seconds.
    #[must_use]
    pub const fn as_seconds(&self) -> u64 {
        match self {
            Self::M1 => 60,
            Self::M5 => 300,
            Self::M15 => 900,
            Self::M30 => 1800,
            Self::H1 => 3600,
            Self::H4 => 14400,
            Self::D1 => 86400,
            Self::W1 => 604800,
        }
    }

    /// Returns the duration as a [`std::time::Duration`].
    #[must_use]
    pub const fn duration(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.as_seconds())
    }

    /// Returns the string representation (e.g., "1m", "1h").
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::M1 => "1m",
            Self::M5 => "5m",
            Self::M15 => "15m",
            Self::M30 => "30m",
            Self::H1 => "1h",
            Self::H4 => "4h",
            Self::D1 => "1d",
            Self::W1 => "1w",
        }
    }

    /// Parses a timeframe from its string representation.
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::values::TimeFrame;
    ///
    /// let tf = TimeFrame::parse("1h").unwrap();
    /// assert_eq!(tf, TimeFrame::H1);
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`ValueError::InvalidTimeFrame`] if the string is not recognized.
    pub fn parse(s: &str) -> Result<Self, ValueError> {
        match s {
            "1m" => Ok(Self::M1),
            "5m" => Ok(Self::M5),
            "15m" => Ok(Self::M15),
            "30m" => Ok(Self::M30),
            "1h" => Ok(Self::H1),
            "4h" => Ok(Self::H4),
            "1d" => Ok(Self::D1),
            "1w" => Ok(Self::W1),
            _ => Err(ValueError::InvalidTimeFrame(format!(
                "Unknown timeframe: {}",
                s
            ))),
        }
    }

    /// Returns all available timeframes.
    #[must_use]
    pub fn all() -> &'static [TimeFrame] {
        &[
            Self::M1,
            Self::M5,
            Self::M15,
            Self::M30,
            Self::H1,
            Self::H4,
            Self::D1,
            Self::W1,
        ]
    }

    /// Returns true if this is an intraday timeframe (< 1 day).
    #[must_use]
    pub const fn is_intraday(&self) -> bool {
        matches!(
            self,
            Self::M1 | Self::M5 | Self::M15 | Self::M30 | Self::H1 | Self::H4
        )
    }
}

impl fmt::Display for TimeFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for TimeFrame {
    type Err = ValueError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Default for TimeFrame {
    fn default() -> Self {
        Self::D1
    }
}

// =============================================================================
// SIDE
// =============================================================================

/// Order side: Buy or Sell.
///
/// Side represents the direction of a trade.
///
/// # Examples
///
/// ```
/// use domain::values::Side;
///
/// let side = Side::Buy;
/// assert_eq!(side.opposite(), Side::Sell);
/// assert_eq!(side.sign(), 1);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    /// Buy side (long position)
    Buy,
    /// Sell side (short position)
    Sell,
}

impl Side {
    /// Returns the opposite side.
    #[must_use]
    pub const fn opposite(&self) -> Self {
        match self {
            Self::Buy => Self::Sell,
            Self::Sell => Self::Buy,
        }
    }

    /// Returns +1 for Buy, -1 for Sell.
    ///
    /// Useful for position size calculations.
    #[must_use]
    pub const fn sign(&self) -> i8 {
        match self {
            Self::Buy => 1,
            Self::Sell => -1,
        }
    }

    /// Returns true if this is a Buy.
    #[must_use]
    pub const fn is_buy(&self) -> bool {
        matches!(self, Self::Buy)
    }

    /// Returns true if this is a Sell.
    #[must_use]
    pub const fn is_sell(&self) -> bool {
        matches!(self, Self::Sell)
    }

    /// Returns the string representation.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Buy => "BUY",
            Self::Sell => "SELL",
        }
    }

    /// Parses a side from its string representation.
    ///
    /// Accepts: "buy", "BUY", "sell", "SELL", "long", "short"
    ///
    /// # Errors
    ///
    /// Returns [`ValueError::InvalidSide`] if the string is not recognized.
    pub fn parse(s: &str) -> Result<Self, ValueError> {
        match s.to_uppercase().as_str() {
            "BUY" | "LONG" | "B" => Ok(Self::Buy),
            "SELL" | "SHORT" | "S" => Ok(Self::Sell),
            _ => Err(ValueError::InvalidSide(format!("Unknown side: {}", s))),
        }
    }
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for Side {
    type Err = ValueError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Default for Side {
    fn default() -> Self {
        Self::Buy
    }
}

// =============================================================================
// ORDER TYPE
// =============================================================================

/// Order type: Market, Limit, Stop, or StopLimit.
///
/// OrderType determines how an order should be executed.
///
/// # Examples
///
/// ```
/// use domain::values::OrderType;
///
/// let order_type = OrderType::Limit;
/// assert!(order_type.requires_limit_price());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OrderType {
    /// Market order (executed immediately at best available price)
    Market,
    /// Limit order (executed at specified price or better)
    Limit,
    /// Stop order (becomes market order when stop price is reached)
    Stop,
    /// Stop-limit order (becomes limit order when stop price is reached)
    StopLimit,
}

impl OrderType {
    /// Returns the string representation.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Market => "MARKET",
            Self::Limit => "LIMIT",
            Self::Stop => "STOP",
            Self::StopLimit => "STOP_LIMIT",
        }
    }

    /// Parses an order type from its string representation.
    ///
    /// Accepts: "market", "MARKET", "limit", "LIMIT", etc.
    ///
    /// # Errors
    ///
    /// Returns [`ValueError::InvalidOrderType`] if the string is not recognized.
    pub fn parse(s: &str) -> Result<Self, ValueError> {
        match s.to_uppercase().as_str() {
            "MARKET" | "MKT" => Ok(Self::Market),
            "LIMIT" | "LMT" => Ok(Self::Limit),
            "STOP" | "STP" => Ok(Self::Stop),
            "STOP_LIMIT" | "STOP-LIMIT" | "STP_LMT" => Ok(Self::StopLimit),
            _ => Err(ValueError::InvalidOrderType(format!(
                "Unknown order type: {}",
                s
            ))),
        }
    }

    /// Returns true if this order type requires a limit price.
    #[must_use]
    pub const fn requires_limit_price(&self) -> bool {
        matches!(self, Self::Limit | Self::StopLimit)
    }

    /// Returns true if this order type requires a stop price.
    #[must_use]
    pub const fn requires_stop_price(&self) -> bool {
        matches!(self, Self::Stop | Self::StopLimit)
    }

    /// Returns true if this is a market order.
    #[must_use]
    pub const fn is_market(&self) -> bool {
        matches!(self, Self::Market)
    }
}

impl fmt::Display for OrderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for OrderType {
    type Err = ValueError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Default for OrderType {
    fn default() -> Self {
        Self::Market
    }
}

// =============================================================================
// TIME IN FORCE
// =============================================================================

/// Time in force: how long an order remains active.
///
/// TimeInForce determines the duration of an order's validity.
///
/// # Examples
///
/// ```
/// use domain::values::TimeInForce;
///
/// let tif = TimeInForce::GTC;
/// assert!(!tif.is_intraday_only());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TimeInForce {
    /// Day order (expires at end of trading day)
    Day,
    /// Good 'til canceled (remains active until filled or canceled)
    GTC,
    /// Immediate or cancel (fill immediately or cancel remaining)
    IOC,
    /// Fill or kill (fill completely immediately or cancel)
    FOK,
}

impl TimeInForce {
    /// Returns the string representation.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Day => "DAY",
            Self::GTC => "GTC",
            Self::IOC => "IOC",
            Self::FOK => "FOK",
        }
    }

    /// Parses a time in force from its string representation.
    ///
    /// Accepts: "day", "DAY", "gtc", "GTC", etc.
    ///
    /// # Errors
    ///
    /// Returns [`ValueError::InvalidTimeInForce`] if the string is not recognized.
    pub fn parse(s: &str) -> Result<Self, ValueError> {
        match s.to_uppercase().as_str() {
            "DAY" => Ok(Self::Day),
            "GTC" | "GOOD_TILL_CANCELED" => Ok(Self::GTC),
            "IOC" | "IMMEDIATE_OR_CANCEL" => Ok(Self::IOC),
            "FOK" | "FILL_OR_KILL" => Ok(Self::FOK),
            _ => Err(ValueError::InvalidTimeInForce(format!(
                "Unknown time in force: {}",
                s
            ))),
        }
    }

    /// Returns true if this TIF is only valid for intraday orders.
    #[must_use]
    pub const fn is_intraday_only(&self) -> bool {
        matches!(self, Self::IOC | Self::FOK)
    }

    /// Returns true if this TIF can persist across multiple days.
    #[must_use]
    pub const fn is_persistent(&self) -> bool {
        matches!(self, Self::GTC)
    }

    /// Returns true if partial fills are allowed.
    #[must_use]
    pub const fn allows_partial_fill(&self) -> bool {
        !matches!(self, Self::FOK)
    }
}

impl fmt::Display for TimeInForce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for TimeInForce {
    type Err = ValueError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Default for TimeInForce {
    fn default() -> Self {
        Self::Day
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    // =========================================================================
    // PRICE TESTS
    // =========================================================================

    #[test]
    fn price_cannot_be_negative() {
        let result = Price::new(dec!(-1.0));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Price must be positive"));
    }

    #[test]
    fn price_cannot_be_zero() {
        let result = Price::new(dec!(0.0));
        assert!(result.is_err());
    }

    #[test]
    fn price_accepts_positive() {
        let price = Price::new(dec!(100.50)).unwrap();
        assert_eq!(price.inner(), dec!(100.50));
    }

    #[test]
    fn price_display_format() {
        let price = Price::new(dec!(100.5)).unwrap();
        assert_eq!(price.to_string(), "100.50");
    }

    #[test]
    fn price_from_str_valid() {
        let price = Price::from_str("123.45").unwrap();
        assert_eq!(price.inner(), dec!(123.45));
    }

    #[test]
    fn price_from_str_invalid_negative() {
        let result = Price::from_str("-10");
        assert!(result.is_err());
    }

    #[test]
    fn price_is_positive() {
        let price = Price::new(dec!(100)).unwrap();
        assert!(price.is_positive());
    }

    #[test]
    fn price_zero_is_not_positive() {
        let price = Price::zero();
        assert!(!price.is_positive());
    }

    #[test]
    fn price_multiplication() {
        let price = Price::new(dec!(100)).unwrap();
        let doubled = price * dec!(2);
        assert_eq!(doubled.inner(), dec!(200));
    }

    // =========================================================================
    // SYMBOL TESTS
    // =========================================================================

    #[test]
    fn symbol_normalizes_to_uppercase() {
        assert_eq!(Symbol::new("nq").unwrap().as_str(), "NQ");
        assert_eq!(Symbol::new("ES").unwrap().as_str(), "ES");
        assert_eq!(Symbol::new("aapl").unwrap().as_str(), "AAPL");
    }

    #[test]
    fn symbol_cannot_be_empty() {
        let result = Symbol::new("");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn symbol_max_length() {
        let long = "A".repeat(21);
        let result = Symbol::new(&long);
        assert!(result.is_err());
    }

    #[test]
    fn symbol_accepts_valid_futures() {
        let sym = Symbol::new("ES/2024").unwrap();
        assert!(sym.is_futures());
        assert!(!sym.is_option());
    }

    #[test]
    fn symbol_accepts_valid_options() {
        let sym = Symbol::new("AAPL:240315C150").unwrap();
        assert!(sym.is_option());
        assert!(!sym.is_futures());
    }

    #[test]
    fn symbol_rejects_invalid_chars() {
        let result = Symbol::new("ES@2024");
        assert!(result.is_err());
    }

    #[test]
    fn symbol_from_str() {
        let sym: Symbol = "msft".parse().unwrap();
        assert_eq!(sym.as_str(), "MSFT");
    }

    // =========================================================================
    // QUANTITY TESTS
    // =========================================================================

    #[test]
    fn quantity_cannot_be_negative() {
        let result = Quantity::new(dec!(-1.0));
        assert!(result.is_err());
    }

    #[test]
    fn quantity_cannot_be_zero() {
        let result = Quantity::new(dec!(0.0));
        assert!(result.is_err());
    }

    #[test]
    fn quantity_accepts_fractional() {
        let qty = Quantity::new(dec!(10.5)).unwrap();
        assert!(!qty.is_integer());
        assert_eq!(qty.inner(), dec!(10.5));
    }

    #[test]
    fn quantity_accepts_integer() {
        let qty = Quantity::new(dec!(100)).unwrap();
        assert!(qty.is_integer());
    }

    #[test]
    fn quantity_floor() {
        let qty = Quantity::new(dec!(10.9)).unwrap();
        assert_eq!(qty.floor().inner(), dec!(10));
    }

    #[test]
    fn quantity_ceil() {
        let qty = Quantity::new(dec!(10.1)).unwrap();
        assert_eq!(qty.ceil().inner(), dec!(11));
    }

    #[test]
    fn quantity_add() {
        let q1 = Quantity::new(dec!(10)).unwrap();
        let q2 = Quantity::new(dec!(5)).unwrap();
        assert_eq!(q1.add(&q2).inner(), dec!(15));
    }

    // =========================================================================
    // VOLUME TESTS
    // =========================================================================

    #[test]
    fn volume_cannot_be_negative() {
        let result = Volume::new(-1);
        assert!(result.is_err());
    }

    #[test]
    fn volume_accepts_zero() {
        let vol = Volume::new(0).unwrap();
        assert!(vol.is_zero());
        assert_eq!(vol.as_i64(), 0);
    }

    #[test]
    fn volume_accepts_positive() {
        let vol = Volume::new(1_000_000).unwrap();
        assert_eq!(vol.as_i64(), 1_000_000);
    }

    #[test]
    fn volume_increment() {
        let vol = Volume::new(100).unwrap();
        let incremented = vol.increment();
        assert_eq!(incremented.as_i64(), 101);
    }

    #[test]
    fn volume_add() {
        let v1 = Volume::new(100).unwrap();
        let v2 = Volume::new(50).unwrap();
        assert_eq!(v1.add(&v2).as_i64(), 150);
    }

    #[test]
    fn volume_from_str() {
        let vol: Volume = "1000".parse().unwrap();
        assert_eq!(vol.as_i64(), 1000);
    }

    // =========================================================================
    // ORDER ID TESTS
    // =========================================================================

    #[test]
    fn order_id_generates_unique() {
        let id1 = OrderId::generate();
        let id2 = OrderId::generate();
        assert_ne!(id1, id2);
    }

    #[test]
    fn order_id_display_format() {
        let id = OrderId::generate();
        let s = id.to_string();
        assert!(s.starts_with("ord-"));
        assert_eq!(s.len(), 40); // "ord-" + 36 char UUID
    }

    #[test]
    fn order_id_from_str_with_prefix() {
        let id_str = "ord-550e8400-e29b-41d4-a716-446655440000";
        let id = OrderId::from_str(id_str).unwrap();
        assert_eq!(id.to_string(), id_str);
    }

    #[test]
    fn order_id_from_str_without_prefix() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let id = OrderId::from_str(uuid_str).unwrap();
        assert_eq!(id.to_simple_string(), uuid_str);
    }

    #[test]
    fn order_id_from_str_invalid() {
        let result = OrderId::from_str("not-a-uuid");
        assert!(result.is_err());
    }

    // =========================================================================
    // CURRENCY TESTS
    // =========================================================================

    #[test]
    fn currency_as_str() {
        assert_eq!(Currency::USD.as_str(), "USD");
        assert_eq!(Currency::EUR.as_str(), "EUR");
        assert_eq!(Currency::JPY.as_str(), "JPY");
    }

    #[test]
    fn currency_decimal_places() {
        assert_eq!(Currency::USD.decimal_places(), 2);
        assert_eq!(Currency::JPY.decimal_places(), 0);
    }

    #[test]
    fn currency_from_str() {
        assert_eq!(Currency::from_str("USD").unwrap(), Currency::USD);
        assert_eq!(Currency::from_str("eur").unwrap(), Currency::EUR);
    }

    #[test]
    fn currency_from_str_invalid() {
        assert!(Currency::from_str("XYZ").is_err());
    }

    // =========================================================================
    // MONEY TESTS
    // =========================================================================

    #[test]
    fn money_creation() {
        let money = Money::new(dec!(100.50), Currency::USD).unwrap();
        assert_eq!(money.amount(), dec!(100.50));
        assert_eq!(money.currency(), Currency::USD);
    }

    #[test]
    fn money_cannot_be_negative() {
        let result = Money::new(dec!(-100), Currency::USD);
        assert!(result.is_err());
    }

    #[test]
    fn money_accepts_zero() {
        let money = Money::new(dec!(0), Currency::EUR).unwrap();
        assert!(money.amount().is_zero());
    }

    #[test]
    fn money_format() {
        let money = Money::new(dec!(100.50), Currency::USD).unwrap();
        assert_eq!(money.format(), "$100.50");

        let eur = Money::new(dec!(50), Currency::EUR).unwrap();
        assert_eq!(eur.format(), "€50.00");

        let jpy = Money::new(dec!(1000), Currency::JPY).unwrap();
        assert_eq!(jpy.format(), "¥1000.00");
    }

    #[test]
    fn money_add_same_currency() {
        let m1 = Money::new(dec!(100), Currency::USD).unwrap();
        let m2 = Money::new(dec!(50), Currency::USD).unwrap();
        let sum = m1.add(&m2).unwrap();
        assert_eq!(sum.amount(), dec!(150));
    }

    #[test]
    fn money_add_different_currency_fails() {
        let m1 = Money::new(dec!(100), Currency::USD).unwrap();
        let m2 = Money::new(dec!(50), Currency::EUR).unwrap();
        assert!(m1.add(&m2).is_err());
    }

    #[test]
    fn money_mul() {
        let money = Money::new(dec!(100), Currency::USD).unwrap();
        let doubled = money.mul(dec!(2));
        assert_eq!(doubled.amount(), dec!(200));
    }

    // =========================================================================
    // TIMEFRAME TESTS
    // =========================================================================

    #[test]
    fn timeframe_as_seconds() {
        assert_eq!(TimeFrame::M1.as_seconds(), 60);
        assert_eq!(TimeFrame::H1.as_seconds(), 3600);
        assert_eq!(TimeFrame::D1.as_seconds(), 86400);
    }

    #[test]
    fn timeframe_as_str() {
        assert_eq!(TimeFrame::M5.as_str(), "5m");
        assert_eq!(TimeFrame::H4.as_str(), "4h");
        assert_eq!(TimeFrame::W1.as_str(), "1w");
    }

    #[test]
    fn timeframe_parse() {
        assert_eq!(TimeFrame::parse("1m").unwrap(), TimeFrame::M1);
        assert_eq!(TimeFrame::parse("1h").unwrap(), TimeFrame::H1);
        assert_eq!(TimeFrame::parse("1d").unwrap(), TimeFrame::D1);
    }

    #[test]
    fn timeframe_parse_invalid() {
        assert!(TimeFrame::parse("10m").is_err());
        assert!(TimeFrame::parse("hour").is_err());
    }

    #[test]
    fn timeframe_is_intraday() {
        assert!(TimeFrame::M1.is_intraday());
        assert!(TimeFrame::H4.is_intraday());
        assert!(!TimeFrame::D1.is_intraday());
        assert!(!TimeFrame::W1.is_intraday());
    }

    #[test]
    fn timeframe_from_str() {
        let tf: TimeFrame = "15m".parse().unwrap();
        assert_eq!(tf, TimeFrame::M15);
    }

    // =========================================================================
    // SIDE TESTS
    // =========================================================================

    #[test]
    fn side_opposite() {
        assert_eq!(Side::Buy.opposite(), Side::Sell);
        assert_eq!(Side::Sell.opposite(), Side::Buy);
    }

    #[test]
    fn side_sign() {
        assert_eq!(Side::Buy.sign(), 1);
        assert_eq!(Side::Sell.sign(), -1);
    }

    #[test]
    fn side_is_buy_sell() {
        assert!(Side::Buy.is_buy());
        assert!(!Side::Buy.is_sell());
        assert!(Side::Sell.is_sell());
        assert!(!Side::Sell.is_buy());
    }

    #[test]
    fn side_parse() {
        assert_eq!(Side::parse("BUY").unwrap(), Side::Buy);
        assert_eq!(Side::parse("buy").unwrap(), Side::Buy);
        assert_eq!(Side::parse("LONG").unwrap(), Side::Buy);
        assert_eq!(Side::parse("SELL").unwrap(), Side::Sell);
        assert_eq!(Side::parse("short").unwrap(), Side::Sell);
        assert_eq!(Side::parse("B").unwrap(), Side::Buy);
    }

    #[test]
    fn side_parse_invalid() {
        assert!(Side::parse("HOLD").is_err());
    }

    #[test]
    fn side_from_str() {
        let side: Side = "SELL".parse().unwrap();
        assert_eq!(side, Side::Sell);
    }

    // =========================================================================
    // ORDER TYPE TESTS
    // =========================================================================

    #[test]
    fn order_type_parse() {
        assert_eq!(OrderType::parse("MARKET").unwrap(), OrderType::Market);
        assert_eq!(OrderType::parse("limit").unwrap(), OrderType::Limit);
        assert_eq!(OrderType::parse("STOP").unwrap(), OrderType::Stop);
        assert_eq!(
            OrderType::parse("STOP-LIMIT").unwrap(),
            OrderType::StopLimit
        );
    }

    #[test]
    fn order_type_requires_limit_price() {
        assert!(!OrderType::Market.requires_limit_price());
        assert!(OrderType::Limit.requires_limit_price());
        assert!(!OrderType::Stop.requires_limit_price());
        assert!(OrderType::StopLimit.requires_limit_price());
    }

    #[test]
    fn order_type_requires_stop_price() {
        assert!(!OrderType::Market.requires_stop_price());
        assert!(!OrderType::Limit.requires_stop_price());
        assert!(OrderType::Stop.requires_stop_price());
        assert!(OrderType::StopLimit.requires_stop_price());
    }

    #[test]
    fn order_type_is_market() {
        assert!(OrderType::Market.is_market());
        assert!(!OrderType::Limit.is_market());
    }

    #[test]
    fn order_type_from_str() {
        let ot: OrderType = "LIMIT".parse().unwrap();
        assert_eq!(ot, OrderType::Limit);
    }

    // =========================================================================
    // TIME IN FORCE TESTS
    // =========================================================================

    #[test]
    fn time_in_force_parse() {
        assert_eq!(TimeInForce::parse("DAY").unwrap(), TimeInForce::Day);
        assert_eq!(TimeInForce::parse("gtc").unwrap(), TimeInForce::GTC);
        assert_eq!(TimeInForce::parse("IOC").unwrap(), TimeInForce::IOC);
        assert_eq!(TimeInForce::parse("FOK").unwrap(), TimeInForce::FOK);
    }

    #[test]
    fn time_in_force_is_intraday_only() {
        assert!(!TimeInForce::Day.is_intraday_only());
        assert!(!TimeInForce::GTC.is_intraday_only());
        assert!(TimeInForce::IOC.is_intraday_only());
        assert!(TimeInForce::FOK.is_intraday_only());
    }

    #[test]
    fn time_in_force_is_persistent() {
        assert!(!TimeInForce::Day.is_persistent());
        assert!(TimeInForce::GTC.is_persistent());
        assert!(!TimeInForce::IOC.is_persistent());
    }

    #[test]
    fn time_in_force_allows_partial_fill() {
        assert!(TimeInForce::Day.allows_partial_fill());
        assert!(TimeInForce::GTC.allows_partial_fill());
        assert!(TimeInForce::IOC.allows_partial_fill());
        assert!(!TimeInForce::FOK.allows_partial_fill());
    }

    #[test]
    fn time_in_force_from_str() {
        let tif: TimeInForce = "GTC".parse().unwrap();
        assert_eq!(tif, TimeInForce::GTC);
    }

    // =========================================================================
    // SERDE ROUNDTRIP TESTS
    // =========================================================================

    #[test]
    fn price_serde_roundtrip() {
        let price = Price::new(dec!(123.45)).unwrap();
        let json = serde_json::to_string(&price).unwrap();
        let decoded: Price = serde_json::from_str(&json).unwrap();
        assert_eq!(price, decoded);
    }

    #[test]
    fn symbol_serde_roundtrip() {
        let symbol = Symbol::new("AAPL").unwrap();
        let json = serde_json::to_string(&symbol).unwrap();
        let decoded: Symbol = serde_json::from_str(&json).unwrap();
        assert_eq!(symbol, decoded);
    }

    #[test]
    fn quantity_serde_roundtrip() {
        let qty = Quantity::new(dec!(100.5)).unwrap();
        let json = serde_json::to_string(&qty).unwrap();
        let decoded: Quantity = serde_json::from_str(&json).unwrap();
        assert_eq!(qty, decoded);
    }

    #[test]
    fn volume_serde_roundtrip() {
        let vol = Volume::new(1000).unwrap();
        let json = serde_json::to_string(&vol).unwrap();
        let decoded: Volume = serde_json::from_str(&json).unwrap();
        assert_eq!(vol, decoded);
    }

    #[test]
    fn order_id_serde_roundtrip() {
        let id = OrderId::generate();
        let json = serde_json::to_string(&id).unwrap();
        let decoded: OrderId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, decoded);
    }

    #[test]
    fn money_serde_roundtrip() {
        let money = Money::new(dec!(100.50), Currency::USD).unwrap();
        let json = serde_json::to_string(&money).unwrap();
        let decoded: Money = serde_json::from_str(&json).unwrap();
        assert_eq!(money, decoded);
    }

    #[test]
    fn timeframe_serde_roundtrip() {
        let tf = TimeFrame::H1;
        let json = serde_json::to_string(&tf).unwrap();
        let decoded: TimeFrame = serde_json::from_str(&json).unwrap();
        assert_eq!(tf, decoded);
    }

    #[test]
    fn side_serde_roundtrip() {
        let side = Side::Buy;
        let json = serde_json::to_string(&side).unwrap();
        let decoded: Side = serde_json::from_str(&json).unwrap();
        assert_eq!(side, decoded);
    }

    #[test]
    fn order_type_serde_roundtrip() {
        let ot = OrderType::Limit;
        let json = serde_json::to_string(&ot).unwrap();
        let decoded: OrderType = serde_json::from_str(&json).unwrap();
        assert_eq!(ot, decoded);
    }

    #[test]
    fn time_in_force_serde_roundtrip() {
        let tif = TimeInForce::GTC;
        let json = serde_json::to_string(&tif).unwrap();
        let decoded: TimeInForce = serde_json::from_str(&json).unwrap();
        assert_eq!(tif, decoded);
    }
}
