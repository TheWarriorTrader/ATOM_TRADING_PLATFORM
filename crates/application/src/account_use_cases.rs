//! # Account Use Cases
//!
//! Use cases for account operations.

use std::sync::Arc;

use tracing::info;

use crate::dto::{AccountDto, CreateAccountDto};
use crate::errors::{ApplicationError, ApplicationResult};
use crate::ports::{NotificationLevel, NotificationPort};
use domain::{AccountRepository, AccountService, Currency};

/// Use case for creating accounts
pub struct CreateAccountUseCase {
    account_service: AccountService,
    account_repo: Arc<dyn AccountRepository>,
    notification_port: Arc<dyn NotificationPort>,
}

impl CreateAccountUseCase {
    /// Creates a new CreateAccountUseCase
    #[must_use]
    pub fn new(
        account_repo: Arc<dyn AccountRepository>,
        notification_port: Arc<dyn NotificationPort>,
    ) -> Self {
        Self {
            account_service: AccountService::new(),
            account_repo,
            notification_port,
        }
    }

    /// Executes the use case
    ///
    /// # Errors
    ///
    /// Returns error if account creation fails
    pub async fn execute(&self, dto: CreateAccountDto) -> ApplicationResult<AccountDto> {
        // Parse currency
        let currency = parse_currency(&dto.currency)?;

        // Create account using domain service
        let account = self
            .account_service
            .create_account(&dto.name, currency, dto.initial_balance)
            .map_err(ApplicationError::from)?;

        // Persist account
        self.account_repo
            .save(&account)
            .await
            .map_err(|e| ApplicationError::repository(e.to_string()))?;

        // Notify
        self.notification_port
            .notify(
                &format!("Account {} created for {}", account.id(), dto.name),
                NotificationLevel::Info,
            )
            .await
            .map_err(|e| ApplicationError::infrastructure(e.to_string()))?;

        info!(
            account_id = %account.id(),
            name = %dto.name,
            "Account created successfully"
        );

        Ok(account_to_dto(&account))
    }
}

/// Use case for retrieving accounts
pub struct GetAccountsUseCase {
    account_repo: Arc<dyn AccountRepository>,
}

impl GetAccountsUseCase {
    /// Creates a new GetAccountsUseCase
    #[must_use]
    pub fn new(account_repo: Arc<dyn AccountRepository>) -> Self {
        Self { account_repo }
    }

    /// Gets all accounts
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    pub async fn get_all(&self) -> ApplicationResult<Vec<AccountDto>> {
        let accounts = self
            .account_repo
            .find_all()
            .await
            .map_err(|e| ApplicationError::repository(e.to_string()))?;

        Ok(accounts.iter().map(account_to_dto).collect())
    }

    /// Gets an account by ID
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    pub async fn get_by_id(&self, account_id: domain::EntityId) -> ApplicationResult<AccountDto> {
        let account = self
            .account_repo
            .find_by_id(account_id)
            .await
            .map_err(|e| ApplicationError::repository(e.to_string()))?
            .ok_or_else(|| ApplicationError::not_found(format!("Account {}", account_id)))?;

        Ok(account_to_dto(&account))
    }
}

/// Use case for updating account balance
pub struct UpdateBalanceUseCase {
    account_service: AccountService,
    account_repo: Arc<dyn AccountRepository>,
    notification_port: Arc<dyn NotificationPort>,
}

impl UpdateBalanceUseCase {
    /// Creates a new UpdateBalanceUseCase
    #[must_use]
    pub fn new(
        account_repo: Arc<dyn AccountRepository>,
        notification_port: Arc<dyn NotificationPort>,
    ) -> Self {
        Self {
            account_service: AccountService::new(),
            account_repo,
            notification_port,
        }
    }

    /// Updates an account's balance
    ///
    /// # Errors
    ///
    /// Returns error if the operation fails
    pub async fn execute(
        &self,
        account_id: domain::EntityId,
        new_balance: rust_decimal::Decimal,
    ) -> ApplicationResult<AccountDto> {
        // Find account
        let mut account = self
            .account_repo
            .find_by_id(account_id)
            .await
            .map_err(|e| ApplicationError::repository(e.to_string()))?
            .ok_or_else(|| ApplicationError::not_found(format!("Account {}", account_id)))?;

        // Update balance using domain service
        self.account_service
            .update_balance(&mut account, new_balance);

        // Persist account
        self.account_repo
            .save(&account)
            .await
            .map_err(|e| ApplicationError::repository(e.to_string()))?;

        // Notify
        self.notification_port
            .notify(
                &format!("Account {} balance updated to {}", account_id, new_balance),
                NotificationLevel::Info,
            )
            .await
            .map_err(|e| ApplicationError::infrastructure(e.to_string()))?;

        info!(
            account_id = %account_id,
            new_balance = %new_balance,
            "Account balance updated"
        );

        Ok(account_to_dto(&account))
    }
}

/// Parses a currency string
fn parse_currency(currency: &str) -> ApplicationResult<Currency> {
    match currency.to_uppercase().as_str() {
        "USD" => Ok(Currency::USD),
        "EUR" => Ok(Currency::EUR),
        "GBP" => Ok(Currency::GBP),
        "JPY" => Ok(Currency::JPY),
        "CHF" => Ok(Currency::CHF),
        _ => Err(ApplicationError::validation(format!(
            "Unsupported currency: {}",
            currency
        ))),
    }
}

/// Converts a domain Account to AccountDto
fn account_to_dto(account: &domain::Account) -> AccountDto {
    AccountDto {
        id: account.id(),
        name: account.name().to_string(),
        currency: account.currency().to_string(),
        balance: account.balance().amount(),
        created_at: account.created_at(),
    }
}
