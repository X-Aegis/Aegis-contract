#![no_std]
#![warn(clippy::all)]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, token,
    Address, Env, Map, Vec,
};

pub mod cross_chain;
mod flash_loan;
use cross_chain::{BridgeEndpoint, CrossChainRebalancePayload, TargetChain};

// ─────────────────────────────────────────────
// Strategy health status
// ─────────────────────────────────────────────
#[contracttype]
#[derive(Clone, PartialEq, Debug)]
/// Represents the operational health status of a strategy.
/// Strategies are marked `Flagged` if their reported balance drops unexpectedly.
pub enum HealthStatus {
    Healthy,
    Flagged,
}

// ─────────────────────────────────────────────
// Error types
// ─────────────────────────────────────────────
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
/// Error codes returned by the Volatility Shield contract.
/// Defines specific failure conditions for protocol interactions.
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    NegativeAmount = 3,
    Unauthorized = 4,
    NoStrategies = 5,
    StrategyNotFound = 6,
    StrategyFlagged = 7,
    ProviderNotWhitelisted = 8,
    ProviderAlreadyWhitelisted = 9,
    ProviderNotFound = 10,
    FeeTooHigh = 11,
    BelowThreshold = 12,
    WithdrawalNotFound = 13,
    WithdrawalAlreadyProcessed = 14,
    NotImplemented = 15,
}

// ─────────────────────────────────────────────
// Storage keys
// ─────────────────────────────────────────────
#[contracttype]
#[derive(Clone)]
/// Storage keys used to persist state within the contract.
/// Each variant corresponds to a unique piece of protocol data.
pub enum DataKey {
    Admin,
    Asset,
    Oracle,
    TotalAssets,
    TotalShares,
    Strategies,
    Treasury,
    FeePercentage,
    Token,
    Balance(Address),
    OracleLastUpdate,
    MaxStaleness,
    OracleAllocations,
    /// Stores HealthStatus for each registered strategy address.
    StrategyHealth(Address),
    /// Whitelist of verified flash-loan providers.
    FlashLoanProviders,
    /// Maximum flash-loan fee accepted, in basis points of the principal.
    MaxFlashLoanFeeBps,
    /// Minimum share amount that triggers queueing instead of instant withdrawal.
    WithdrawQueueThreshold,
    /// Map of queued withdrawal IDs to PendingWithdrawal data.
    PendingWithdrawals,
    /// Monotonically incrementing counter for withdrawal IDs.
    WithdrawQueueCounter,
    /// Annualised percentage yield against which performance is measured (BPS, default 500 = 5%).
    BenchmarkRate,
    /// Current vault APY in BPS set by the oracle each epoch.
    CurrentVaultApy,
    /// List of registered cross-chain bridge endpoints.
    CrossChainEndpoints,
    /// Counter for cross-chain bridge endpoint IDs.
    CrossChainEndpointCounter,
    /// Monotonically incrementing nonce for CrossChainRebalance payloads.
    CrossChainRebalanceNonce,
    /// Governance token address for future governance integration.
    GovernanceToken,
}

// ─────────────────────────────────────────────
// Strategy cross-contract client
// ─────────────────────────────────────────────
/// Client wrapper for interacting with registered strategy contracts.
/// Exposes `deposit`, `withdraw`, and `balance` cross-contract calls.
pub struct StrategyClient<'a> {
    env: &'a Env,
    address: Address,
}

// ── Withdrawal Queue ───────────────────────

/// A pending withdrawal that has been queued because it exceeds the threshold.
#[contracttype]
#[derive(Clone, PartialEq, Debug)]
pub struct PendingWithdrawal {
    pub from: Address,
    pub shares: i128,
    pub created_at: u64,
    pub processed: bool,
}

impl<'a> StrategyClient<'a> {
    /// Creates a new `StrategyClient` instance for the given address.
    ///
    /// # Arguments
    /// * `env` - The execution environment.
    /// * `address` - The address of the strategy contract.
    pub fn new(env: &'a Env, address: Address) -> Self {
        Self { env, address }
    }

    /// Deposits the specified `amount` into the strategy.
    ///
    /// # Arguments
    /// * `amount` - The positive integer amount to deposit.
    pub fn deposit(&self, amount: i128) {
        self.env.invoke_contract::<()>(
            &self.address,
            &soroban_sdk::Symbol::new(self.env, "deposit"),
            soroban_sdk::vec![self.env, soroban_sdk::IntoVal::into_val(&amount, self.env)],
        );
    }

    /// Withdraws the specified `amount` from the strategy.
    ///
    /// # Arguments
    /// * `amount` - The positive integer amount to withdraw.
    pub fn withdraw(&self, amount: i128) {
        self.env.invoke_contract::<()>(
            &self.address,
            &soroban_sdk::Symbol::new(self.env, "withdraw"),
            soroban_sdk::vec![self.env, soroban_sdk::IntoVal::into_val(&amount, self.env)],
        );
    }

    /// Queries the current balance of the strategy.
    ///
    /// # Returns
    /// The current balance as an `i128`.
    pub fn balance(&self) -> i128 {
        self.env.invoke_contract::<i128>(
            &self.address,
            &soroban_sdk::Symbol::new(self.env, "balance"),
            soroban_sdk::vec![self.env],
        )
    }
}

// ─────────────────────────────────────────────
// Contract
// ─────────────────────────────────────────────
#[contract]
/// The main contract struct for the Volatility Shield.
/// Orchestrates user deposits, withdrawals, and oracle-driven rebalancing.
pub struct VolatilityShield;

#[contractimpl]
impl VolatilityShield {
    // ── Initialization ────────────────────────
    /// Must be called once. Stores roles and configuration.
    pub fn init(
        env: Env,
        admin: Address,
        asset: Address,
        oracle: Address,
        treasury: Address,
        fee_percentage: u32,
    ) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Asset, &asset);
        env.storage().instance().set(&DataKey::Oracle, &oracle);
        env.storage()
            .instance()
            .set(&DataKey::Strategies, &Vec::<Address>::new(&env));
        env.storage().instance().set(&DataKey::Treasury, &treasury);
        env.storage()
            .instance()
            .set(&DataKey::FeePercentage, &fee_percentage);
        env.storage().instance().set(&DataKey::Token, &asset);
    }

    // ── Deposit ───────────────────────────────
    /// Deposits assets into the vault, minting shares for the depositor.
    ///
    /// # Arguments
    /// * `env` - The execution environment.
    /// * `from` - The address making the deposit.
    /// * `amount` - The quantity of the underlying asset to deposit.
    ///
    /// # Panics
    /// * If `amount` is <= 0.
    /// * If `from` does not authorize the invocation.
    pub fn deposit(env: Env, from: Address, amount: i128) {
        if amount <= 0 {
            panic!("deposit amount must be positive");
        }
        from.require_auth();

        let token: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .expect("Token not initialized");
        token::Client::new(&env, &token).transfer(&from, &env.current_contract_address(), &amount);

        let shares_to_mint = Self::convert_to_shares(env.clone(), amount);

        let balance_key = DataKey::Balance(from.clone());
        let current_balance: i128 = env.storage().persistent().get(&balance_key).unwrap_or(0);
        env.storage().persistent().set(
            &balance_key,
            &(current_balance.checked_add(shares_to_mint).unwrap()),
        );

        let total_shares = Self::total_shares(&env);
        let total_assets = Self::total_assets(&env);
        let new_total_shares = total_shares.checked_add(shares_to_mint).unwrap();
        let new_total_assets = total_assets.checked_add(amount).unwrap();
        Self::set_total_shares(env.clone(), new_total_shares);
        Self::set_total_assets(env.clone(), new_total_assets);

        let share_price_at_time = if total_shares == 0 { 10_000_000 } else { total_assets * 10_000_000 / total_shares };
        env.events().publish(
            (soroban_sdk::Symbol::new(&env, "Deposit"), from.clone()),
            (amount, shares_to_mint, new_total_assets, new_total_shares, share_price_at_time)
        );
    }

    // ── Withdraw ──────────────────────────────
    /// Withdraws assets from the vault by burning the specified `shares`.
    ///
    /// # Arguments
    /// * `env` - The execution environment.
    /// * `from` - The address initiating the withdrawal.
    /// * `shares` - The amount of shares to burn.
    ///
    /// # Panics
    /// * If `shares` is <= 0.
    /// * If `from` does not authorize the invocation.
    /// * If `from` lacks sufficient shares.
    pub fn withdraw(env: Env, from: Address, shares: i128) {
        if shares <= 0 {
            panic!("shares to withdraw must be positive");
        }
        from.require_auth();

        let balance_key = DataKey::Balance(from.clone());
        let current_balance: i128 = env.storage().persistent().get(&balance_key).unwrap_or(0);

        if current_balance < shares {
            panic!("insufficient shares for withdrawal");
        }

        let assets_to_withdraw = Self::convert_to_assets(env.clone(), shares);
        let (net_assets, fee) = Self::take_fees(&env, assets_to_withdraw);

        let total_shares = Self::total_shares(&env);
        let total_assets = Self::total_assets(&env);

        let new_total_shares = total_shares.checked_sub(shares).unwrap();
        let new_total_assets = total_assets.checked_sub(assets_to_withdraw).unwrap();


        Self::set_total_shares(env.clone(), new_total_shares);
        Self::set_total_assets(env.clone(), new_total_assets);
        env.storage().persistent().set(
            &balance_key,
            &(current_balance.checked_sub(shares).unwrap()),
        );

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .expect("Token not initialized");
        let token_client = token::Client::new(&env, &token_addr);
        let contract_addr = env.current_contract_address();

        // 1. Transfer net assets to user
        token_client.transfer(&contract_addr, &from, &net_assets);

        // 2. Transfer fee to treasury if any
        if fee > 0 {
            let treasury_addr = Self::treasury(&env);
            token_client.transfer(&contract_addr, &treasury_addr, &fee);
            env.events().publish((soroban_sdk::Symbol::new(&env, "Fee"), soroban_sdk::Symbol::new(&env, "Collect")), fee);
        }

        let share_price_at_time = if total_shares == 0 { 10_000_000 } else { total_assets * 10_000_000 / total_shares };
        env.events().publish(
            (soroban_sdk::Symbol::new(&env, "Withdraw"), from.clone()),
            (shares, net_assets, fee, new_total_assets, new_total_shares, share_price_at_time)
        );

    }

    // ── Withdrawal Queue ───────────────────────

    /// Queue a withdrawal when `shares` exceeds the configured threshold.
    /// The withdrawal is stored in the pending queue and must be processed
    /// by the admin/oracle via `process_queued_withdrawal`.
    pub fn queue_withdraw(env: Env, from: Address, shares: i128) -> Result<u32, Error> {
        if shares <= 0 {
            panic!("shares to withdraw must be positive");
        }
        from.require_auth();

        let balance_key = DataKey::Balance(from.clone());
        let current_balance: i128 = env.storage().persistent().get(&balance_key).unwrap_or(0);
        if current_balance < shares {
            panic!("insufficient shares for withdrawal");
        }

        let threshold: i128 = env
            .storage()
            .instance()
            .get(&DataKey::WithdrawQueueThreshold)
            .unwrap_or(0);

        if threshold > 0 && shares < threshold {
            return Err(Error::BelowThreshold);
        }

        let counter: u32 = env
            .storage()
            .instance()
            .get(&DataKey::WithdrawQueueCounter)
            .unwrap_or(0);
        let withdrawal_id = counter + 1;
        env.storage()
            .instance()
            .set(&DataKey::WithdrawQueueCounter, &withdrawal_id);

        let pending = PendingWithdrawal {
            from: from.clone(),
            shares,
            created_at: env.ledger().timestamp(),
            processed: false,
        };

        env.storage()
            .persistent()
            .set(&DataKey::PendingWithdrawals, &pending);

        // Deduct shares immediately so they cannot be double-spent
        env.storage().persistent().set(
            &balance_key,
            &(current_balance.checked_sub(shares).unwrap()),
        );

        env.events()
            .publish((symbol_short!("Withdraw"), symbol_short!("queued")), shares);

        Ok(withdrawal_id)
    }

    /// Set the withdrawal queue threshold. Admin-only.
    pub fn set_withdraw_queue_threshold(env: Env, caller: Address, threshold: i128) {
        caller.require_auth();
        let admin = Self::get_admin(&env);
        if caller != admin {
            panic!("Unauthorized");
        }
        env.storage()
            .instance()
            .set(&DataKey::WithdrawQueueThreshold, &threshold);
    }

    /// Read the current withdrawal queue threshold.
    pub fn get_withdraw_queue_threshold(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::WithdrawQueueThreshold)
            .unwrap_or(0)
    }

    // ── Oracle Data ──────────────────────────────
    /// Configures the maximum allowed staleness for oracle data.
    ///
    /// # Arguments
    /// * `env` - The execution environment.
    /// * `caller` - The admin address making the change.
    /// * `max_staleness` - The staleness threshold in seconds.
    ///
    /// # Panics
    /// * If `caller` does not authorize the invocation.
    /// * If `caller` is not the admin.
    pub fn set_max_staleness(env: Env, caller: Address, max_staleness: u64) {
        caller.require_auth();
        let admin = Self::get_admin(&env);
        if caller != admin {
            panic!("Unauthorized");
        }
        env.storage()
            .instance()
            .set(&DataKey::MaxStaleness, &max_staleness);
    }

    /// Pushes new strategy allocation weights from the oracle.
    ///
    /// # Arguments
    /// * `env` - The execution environment.
    /// * `caller` - The oracle address making the update.
    /// * `allocations` - A map of strategy addresses to their target allocation in basis points.
    /// * `timestamp` - The timestamp of the oracle data.
    ///
    /// # Panics
    /// * If `caller` does not authorize the invocation.
    /// * If `caller` is not the authorized oracle.
    /// * If the data is stale beyond `max_staleness`.
    pub fn set_oracle_data(
        env: Env,
        caller: Address,
        allocations: Map<Address, i128>,
        timestamp: u64,
    ) {
        caller.require_auth();
        let oracle = Self::get_oracle(&env);
        if caller != oracle {
            panic!("Unauthorized");
        }

        let current_time = env.ledger().timestamp();
        let max_staleness: u64 = env
            .storage()
            .instance()
            .get(&DataKey::MaxStaleness)
            .unwrap_or(3600);

        if current_time > timestamp && current_time - timestamp > max_staleness {
            env.events().publish(
                (symbol_short!("Oracle"), symbol_short!("Reject")),
                timestamp,
            );
            panic!("Stale oracle data");
        }

        env.storage()
            .instance()
            .set(&DataKey::OracleLastUpdate, &timestamp);
        env.storage()
            .instance()
            .set(&DataKey::OracleAllocations, &allocations);
    }

    fn validate_allocations(env: &Env, allocations: &Map<Address, i128>) {
        let mut total_bps: i128 = 0;
        let whitelist = Self::get_strategies(env);
        for (strategy_addr, alloc_bps) in allocations.iter() {
            if alloc_bps < 0 {
                panic!("allocation values cannot be negative");
            }
            if !whitelist.contains(strategy_addr) {
                panic!("allocation to zero-address or unlisted strategy");
            }
            total_bps += alloc_bps;
        }
        if total_bps != 10000 {
            panic!("allocation percentages must sum to 10000 BPS");
        }
    }

    // ── Rebalance ─────────────────────────────
    /// Move funds between strategies according to stored `allocations`.
    pub fn rebalance(env: Env, caller: Address) {
        caller.require_auth();
        let admin = Self::get_admin(&env);
        let oracle = Self::get_oracle(&env);

        if caller != admin && caller != oracle {
            panic!("Unauthorized");
        }

        let current_time = env.ledger().timestamp();
        let last_update: u64 = env
            .storage()
            .instance()
            .get(&DataKey::OracleLastUpdate)
            .expect("No oracle data");
        let max_staleness: u64 = env
            .storage()
            .instance()
            .get(&DataKey::MaxStaleness)
            .unwrap_or(3600);

        if current_time > last_update && current_time - last_update > max_staleness {
            env.events().publish(
                (symbol_short!("Oracle"), symbol_short!("Reject")),
                last_update,
            );
            panic!("Stale oracle data");
        }

        let allocations: Map<Address, i128> = env
            .storage()
            .instance()
            .get(&DataKey::OracleAllocations)
            .expect("No allocations");

        Self::validate_allocations(&env, &allocations);

        let asset_addr = Self::get_asset(&env);
        let token_client = token::Client::new(&env, &asset_addr);
        let vault = env.current_contract_address();
        let total_assets = Self::total_assets(&env);

        for (strategy_addr, alloc_bps) in allocations.iter() {
            let target_allocation = (total_assets * alloc_bps) / 10000;
            let strategy = StrategyClient::new(&env, strategy_addr.clone());
            let current_balance = strategy.balance();

            if target_allocation > current_balance {
                let diff = target_allocation - current_balance;
                token_client.transfer(&vault, &strategy_addr, &diff);
                strategy.deposit(diff);
            } else if target_allocation < current_balance {
                let diff = current_balance - target_allocation;
                strategy.withdraw(diff);
                token_client.transfer(&strategy_addr, &vault, &diff);
            }
        }
        
        let final_assets = Self::total_assets(&env);
        let final_shares = Self::total_shares(&env);
        env.events().publish((soroban_sdk::Symbol::new(&env, "Rebalance"),), allocations);
        env.events().publish((soroban_sdk::Symbol::new(&env, "VaultSnapshot"),), (final_assets, final_shares));
    }

    // ── Strategy Management ───────────────────
    /// Registers a new strategy with the vault.
    ///
    /// # Arguments
    /// * `env` - The execution environment.
    /// * `strategy` - The address of the strategy to add.
    ///
    /// # Returns
    /// * `Ok(())` on success.
    /// * `Err(Error::AlreadyInitialized)` if the strategy is already registered.
    pub fn add_strategy(env: Env, strategy: Address) -> Result<(), Error> {
        let admin = Self::get_admin(&env);
        admin.require_auth();

        let mut strategies: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Strategies)
            .unwrap_or(Vec::new(&env));
        if strategies.contains(strategy.clone()) {
            return Err(Error::AlreadyInitialized);
        }
        strategies.push_back(strategy.clone());
        env.storage()
            .instance()
            .set(&DataKey::Strategies, &strategies);

        env.events().publish(
            (symbol_short!("Strategy"), symbol_short!("added")),
            strategy,
        );

        Ok(())
    }

    // ── Strategy Health ───────────────────────

    /// Checks all registered strategies by comparing their reported balance
    /// against the last-known balance stored in StrategyHealth.  
    /// A strategy whose balance has dropped to zero while a prior balance was
    /// recorded is automatically flagged.
    /// Returns a `Vec` of currently-flagged strategy addresses.
    pub fn check_strategy_health(env: Env) -> Vec<Address> {
        let strategies = Self::get_strategies(&env);
        let mut flagged: Vec<Address> = Vec::new(&env);

        for strategy_addr in strategies.iter() {
            let health_key = DataKey::StrategyHealth(strategy_addr.clone());
            let last_known: i128 = env.storage().persistent().get(&health_key).unwrap_or(0);

            let actual: i128 = env.invoke_contract(
                &strategy_addr,
                &soroban_sdk::Symbol::new(&env, "balance"),
                soroban_sdk::vec![&env],
            );

            // Update the stored balance to the latest reading
            env.storage().persistent().set(&health_key, &actual);

            // Flag the strategy if it had a positive expected balance but
            // now reports zero (or if it is already flagged)
            if actual == 0 && last_known > 0 {
                env.events().publish(
                    (symbol_short!("Strategy"), symbol_short!("flagged")),
                    strategy_addr.clone(),
                );
                // Persist flagged status as -1 sentinel
                env.storage().persistent().set(&health_key, &(-1i128));
                flagged.push_back(strategy_addr.clone());
            }
        }

        flagged
    }

    /// Flag a strategy as unhealthy. Admin-only.
    /// Emits a `StrategyFlagged` event.
    pub fn flag_strategy(env: Env, caller: Address, strategy: Address) -> Result<(), Error> {
        caller.require_auth();
        let admin = Self::get_admin(&env);
        if caller != admin {
            return Err(Error::Unauthorized);
        }

        let strategies = Self::get_strategies(&env);
        if !strategies.contains(strategy.clone()) {
            return Err(Error::StrategyNotFound);
        }

        // Store -1 as a sentinel meaning "flagged"
        env.storage()
            .persistent()
            .set(&DataKey::StrategyHealth(strategy.clone()), &(-1i128));

        env.events().publish(
            (symbol_short!("Strategy"), symbol_short!("flagged")),
            strategy,
        );

        Ok(())
    }

    /// Remove a strategy: withdraws all remaining funds first, then de-lists it.
    /// Admin-only. Emits a `StrategyRemoved` event.
    pub fn remove_strategy(env: Env, caller: Address, strategy: Address) -> Result<(), Error> {
        caller.require_auth();
        let admin = Self::get_admin(&env);
        if caller != admin {
            return Err(Error::Unauthorized);
        }

        let mut strategies: Vec<Address> = Self::get_strategies(&env);
        let mut found_idx: Option<u32> = None;
        for (i, s) in strategies.iter().enumerate() {
            if s == strategy {
                found_idx = Some(i as u32);
                break;
            }
        }

        let idx = found_idx.ok_or(Error::StrategyNotFound)?;

        // Withdraw all remaining funds from the strategy
        let strategy_client = StrategyClient::new(&env, strategy.clone());
        let remaining = strategy_client.balance();
        if remaining > 0 {
            strategy_client.withdraw(remaining);
            // Reduce tracked total_assets accordingly
            let current_assets = Self::total_assets(&env);
            let new_assets = if current_assets >= remaining {
                current_assets - remaining
            } else {
                0
            };
            env.storage()
                .instance()
                .set(&DataKey::TotalAssets, &new_assets);
        }

        // Remove from the strategies list
        strategies.remove(idx);
        env.storage()
            .instance()
            .set(&DataKey::Strategies, &strategies);

        // Clean up health entry
        env.storage()
            .persistent()
            .remove(&DataKey::StrategyHealth(strategy.clone()));

        env.events().publish(
            (symbol_short!("Strategy"), symbol_short!("removed")),
            strategy,
        );

        Ok(())
    }

    // ── Strategy Health View ──────────────────

    /// Returns the raw stored health value for a strategy.
    /// -1  → flagged
    ///  0  → no history recorded yet
    /// >0  → last recorded balance
    pub fn get_strategy_health(env: Env, strategy: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::StrategyHealth(strategy))
            .unwrap_or(0)
    }

    /// Harvests accumulated yield across all registered strategies.
    ///
    /// # Arguments
    /// * `env` - The execution environment.
    ///
    /// # Returns
    /// * `Ok(total_yield)` containing the total harvested amount.
    /// * `Err(Error::NoStrategies)` if no strategies are registered.
    pub fn harvest(env: Env) -> Result<i128, Error> {
        let admin = Self::get_admin(&env);
        admin.require_auth();

        let strategies = Self::get_strategies(&env);
        if strategies.is_empty() {
            return Err(Error::NoStrategies);
        }

        let mut total_yield: i128 = 0;
        for strategy_addr in strategies.iter() {
            let strategy = StrategyClient::new(&env, strategy_addr);
            let yield_amount = strategy.balance();
            total_yield = total_yield.checked_add(yield_amount).unwrap();
        }

        if total_yield > 0 {
            let current_assets = Self::total_assets(&env);
            Self::set_total_assets(
                env.clone(),
                current_assets.checked_add(total_yield).unwrap(),
            );
        }

        let final_assets = Self::total_assets(&env);
        let final_shares = Self::total_shares(&env);
        env.events().publish((soroban_sdk::Symbol::new(&env, "Harvest"),), (total_yield, final_assets));
        env.events().publish((soroban_sdk::Symbol::new(&env, "VaultSnapshot"),), (final_assets, final_shares));
        Ok(total_yield)
    }

    // ── Flash Loan Support (SC-32) ────────────
    //
    // Lets the admin/oracle pull liquidity from a *whitelisted* flash-loan
    // provider to rebalance, then repays it atomically within the same
    // transaction. See flash_loan.rs for the security model.

    /// Default cap on a provider's fee: 1% (100 bps) of the borrowed principal.
    const DEFAULT_MAX_FLASH_LOAN_FEE_BPS: u32 = 100;

    /// Whitelist a verified flash-loan provider. Admin-only.
    pub fn add_flash_loan_provider(
        env: Env,
        caller: Address,
        provider: Address,
    ) -> Result<(), Error> {
        caller.require_auth();
        if caller != Self::get_admin(&env) {
            return Err(Error::Unauthorized);
        }

        let mut providers = Self::get_flash_loan_providers(env.clone());
        if providers.contains(provider.clone()) {
            return Err(Error::ProviderAlreadyWhitelisted);
        }
        providers.push_back(provider.clone());
        env.storage()
            .instance()
            .set(&DataKey::FlashLoanProviders, &providers);

        env.events().publish(
            (symbol_short!("FLProvdr"), symbol_short!("added")),
            provider,
        );
        Ok(())
    }

    /// Remove a flash-loan provider from the whitelist. Admin-only.
    pub fn remove_flash_loan_provider(
        env: Env,
        caller: Address,
        provider: Address,
    ) -> Result<(), Error> {
        caller.require_auth();
        if caller != Self::get_admin(&env) {
            return Err(Error::Unauthorized);
        }

        let mut providers = Self::get_flash_loan_providers(env.clone());
        let mut found: Option<u32> = None;
        for (i, p) in providers.iter().enumerate() {
            if p == provider {
                found = Some(i as u32);
                break;
            }
        }
        let idx = found.ok_or(Error::ProviderNotFound)?;
        providers.remove(idx);
        env.storage()
            .instance()
            .set(&DataKey::FlashLoanProviders, &providers);

        env.events().publish(
            (symbol_short!("FLProvdr"), symbol_short!("removed")),
            provider,
        );
        Ok(())
    }

    /// Returns the list of whitelisted flash-loan providers.
    pub fn get_flash_loan_providers(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::FlashLoanProviders)
            .unwrap_or(Vec::new(&env))
    }

    /// Whether `provider` is a whitelisted flash-loan provider.
    pub fn is_flash_loan_provider(env: Env, provider: Address) -> bool {
        Self::get_flash_loan_providers(env).contains(provider)
    }

    /// Set the maximum acceptable flash-loan fee, in basis points. Admin-only.
    pub fn set_max_flash_loan_fee_bps(env: Env, caller: Address, bps: u32) -> Result<(), Error> {
        caller.require_auth();
        if caller != Self::get_admin(&env) {
            return Err(Error::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&DataKey::MaxFlashLoanFeeBps, &bps);
        Ok(())
    }

    /// Maximum accepted flash-loan fee in basis points (defaults to 1%).
    pub fn max_flash_loan_fee_bps(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::MaxFlashLoanFeeBps)
            .unwrap_or(Self::DEFAULT_MAX_FLASH_LOAN_FEE_BPS)
    }

    /// Flash-loan callback — the vault's receiver hook, invoked by a
    /// whitelisted provider after it lends `amount` of `token` to the vault.
    ///
    /// Soroban forbids contract re-entry, so the flash loan is **provider-
    /// initiated**: an admin-authorized transaction calls the provider, which
    /// lends to the vault and then calls this function (the vault appears only
    /// once on the call stack). The vault uses the borrowed liquidity for the
    /// rebalance window and repays `amount + fee` to the provider with a token
    /// transfer (never a call back into the provider — so no re-entry). The
    /// provider verifies repayment after this returns; if anything here traps,
    /// the whole transaction — including the lend — reverts.
    ///
    /// Guards:
    /// - `admin.require_auth()` — only an admin-authorized rebalance can pull a
    ///   flash loan, so the callback can't be invoked by an arbitrary caller to
    ///   drain the vault.
    /// - `initiator` (the provider) must be whitelisted — the vault only ever
    ///   repays known, verified providers.
    /// - the fee must not exceed the configured cap.
    pub fn flash_loan_callback(
        env: Env,
        token: Address,
        amount: i128,
        fee: i128,
        initiator: Address,
    ) {
        // Only an admin-authorized rebalance may use a flash loan.
        Self::get_admin(&env).require_auth();

        // The vault only repays verified, whitelisted providers.
        if !Self::get_flash_loan_providers(env.clone()).contains(initiator.clone()) {
            panic_with_error!(&env, Error::ProviderNotWhitelisted);
        }
        if amount <= 0 || fee < 0 {
            panic_with_error!(&env, Error::NegativeAmount);
        }
        let max_fee = amount
            .checked_mul(Self::max_flash_loan_fee_bps(&env) as i128)
            .unwrap()
            .checked_div(10000)
            .unwrap();
        if fee > max_fee {
            panic_with_error!(&env, Error::FeeTooHigh);
        }

        // Rebalance window: the borrowed `amount` is now held by the vault and
        // available to the rebalance flow before being repaid in this same tx.
        env.events()
            .publish((symbol_short!("FlashLn"), symbol_short!("rebal")), amount);

        // Repay principal + fee to the provider atomically (a token transfer,
        // not a call into the provider, so the vault is never re-entered).
        let repayment = amount.checked_add(fee).unwrap();
        token::Client::new(&env, &token).transfer(
            &env.current_contract_address(),
            &initiator,
            &repayment,
        );

        env.events().publish(
            (symbol_short!("FlashLn"), symbol_short!("repaid")),
            repayment,
        );
    }

    // ── View helpers ──────────────────────────
    /// Returns the total amount of underlying assets managed by the vault.
    pub fn total_assets(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalAssets)
            .unwrap_or(0)
    }

    /// Returns the total amount of shares currently minted by the vault.
    pub fn total_shares(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalShares)
            .unwrap_or(0)
    }

    /// Returns the address of the current admin.
    pub fn get_admin(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized")
    }

    /// Returns the address of the current oracle.
    pub fn get_oracle(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Oracle)
            .expect("Not initialized")
    }

    /// Returns the address of the primary underlying asset.
    pub fn get_asset(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Asset)
            .expect("Not initialized")
    }

    /// Returns a list of all registered strategy addresses.
    pub fn get_strategies(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::Strategies)
            .unwrap_or(Vec::new(env))
    }

    /// Returns the address of the treasury fee recipient.
    pub fn treasury(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Treasury)
            .expect("Not initialized")
    }

    /// Returns the current withdrawal fee percentage in basis points.
    pub fn fee_percentage(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::FeePercentage)
            .unwrap_or(0)
    }

    /// Queries the current balance of the strategy.
    ///
    /// # Returns
    /// The current balance as an `i128`.
    pub fn balance(env: Env, user: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Balance(user))
            .unwrap_or(0)
    }

    /// Write the benchmark yield rate used for dynamic fee calculation.
    /// Stored in basis points (e.g. 500 = 5%). Admin-only.
    pub fn set_benchmark_rate(env: Env, caller: Address, bps: u32) {
        caller.require_auth();
        if caller != Self::get_admin(&env) {
            panic!("Unauthorized");
        }
        env.storage().instance().set(&DataKey::BenchmarkRate, &bps);
        env.events()
            .publish((symbol_short!("Benchmark"), symbol_short!("set")), bps);
    }

    /// Read the configured benchmark rate in BPS (defaults to 0).
    pub fn benchmark_rate(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::BenchmarkRate)
            .unwrap_or(0)
    }

    /// Write the current vault APY (BPS) used for dynamic fee scaling.
    /// Set by the oracle each epoch. Admin-only.
    pub fn set_current_vault_apy(env: Env, caller: Address, bps: u32) {
        caller.require_auth();
        if caller != Self::get_admin(&env) {
            panic!("Unauthorized");
        }
        env.storage()
            .instance()
            .set(&DataKey::CurrentVaultApy, &bps);
        env.events()
            .publish((symbol_short!("VaultApy"), symbol_short!("set")), bps);
    }

    /// Read the current vault APY in BPS (defaults to 0).
    pub fn current_vault_apy(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::CurrentVaultApy)
            .unwrap_or(0)
    }

    // ── Internal Helpers ──────────────────────
    /// Calculates and deducts the withdrawal fee from the provided asset amount, returning the net assets and fee taken.
    pub fn take_fees(env: &Env, amount: i128) -> (i128, i128) {
        let fee_pct = Self::effective_fee_pct(env);
        if fee_pct == 0 {
            return (amount, 0);
        }
        let fee = amount
            .checked_mul(fee_pct as i128)
            .unwrap()
            .checked_div(10000)
            .unwrap();
        (amount - fee, fee)
    }

    /// Returns the dynamic fee percentage (BPS) scaled by performance vs benchmark.
    /// - Outperforming (apy > benchmark): fee scales up linearly, capped at 2× base
    /// - Underperforming (apy < benchmark): fee scales down, floored at 0.5× base
    /// - No benchmark data → falls back to base fee
    fn effective_fee_pct(env: &Env) -> u32 {
        let base = Self::fee_percentage(env);
        if base == 0 {
            return 0;
        }

        let vault_apy = env
            .storage()
            .instance()
            .get(&DataKey::CurrentVaultApy)
            .unwrap_or(0u32);
        let benchmark = env
            .storage()
            .instance()
            .get(&DataKey::BenchmarkRate)
            .unwrap_or(0u32);

        if benchmark == 0 || vault_apy == 0 {
            return base;
        }

        // Scale: fee = base * (1 + (vault_apy - benchmark) / benchmark)
        // Multiplier clamped between 0.5× and 2.0×
        let ratio: u64 = (vault_apy as u64)
            .checked_mul(10000)
            .unwrap_or(0)
            .checked_div(benchmark as u64)
            .unwrap_or(10000);

        // Convert ratio (10000 = 1.0) to multiplier in basis points
        let multiplier_bps = if ratio > 10000 {
            // vault_apy > benchmark → fee increases
            let excess = ratio - 10000;
            // Clamp excess: at most 100% above benchmark gives 2× base
            let capped_excess = if excess > 10000 { 10000 } else { excess };
            10000u64 + capped_excess
        } else {
            // vault_apy < benchmark → fee decreases
            let deficit = 10000 - ratio;
            let capped_deficit = if deficit > 5000 { 5000 } else { deficit };
            10000u64 - capped_deficit
        };

        let effective: u64 = (base as u64)
            .checked_mul(multiplier_bps)
            .unwrap_or(0)
            .checked_div(10000)
            .unwrap_or(base as u64);

        effective as u32
    }

    /// Converts an amount of underlying assets to the equivalent number of shares, rounding down.
    pub fn convert_to_shares(env: Env, amount: i128) -> i128 {
        if amount < 0 {
            panic!("negative amount");
        }
        let total_shares = Self::total_shares(&env);
        let total_assets = Self::total_assets(&env);
        if total_shares == 0 || total_assets == 0 {
            return amount;
        }
        amount
            .checked_mul(total_shares)
            .unwrap()
            .checked_div(total_assets)
            .unwrap()
    }

    /// Converts an amount of shares to the equivalent amount of underlying assets, rounding down.
    pub fn convert_to_assets(env: Env, shares: i128) -> i128 {
        if shares < 0 {
            panic!("negative amount");
        }
        let total_shares = Self::total_shares(&env);
        let total_assets = Self::total_assets(&env);
        if total_shares == 0 {
            return shares;
        }
        shares
            .checked_mul(total_assets)
            .unwrap()
            .checked_div(total_shares)
            .unwrap()
    }

    /// Internally updates the total assets storage.
    pub fn set_total_assets(env: Env, amount: i128) {
        env.storage().instance().set(&DataKey::TotalAssets, &amount);
    }

    /// Internally updates the total shares storage.
    pub fn set_total_shares(env: Env, amount: i128) {
        env.storage().instance().set(&DataKey::TotalShares, &amount);
    }

    /// Internally sets the share balance of a specific user.
    pub fn set_balance(env: Env, user: Address, amount: i128) {
        env.storage()
            .persistent()
            .set(&DataKey::Balance(user), &amount);
    }

    /// Internally updates the token address storage.
    pub fn set_token(env: Env, token: Address) {
        env.storage().instance().set(&DataKey::Token, &token);
    }

    // ── Cross-Chain Rebalance Messaging (SC-33) ──────────

    /// Register a cross-chain bridge endpoint. Admin-only.
    /// Returns the assigned endpoint ID.
    pub fn add_cross_chain_endpoint(
        env: Env,
        caller: Address,
        chain: TargetChain,
        destination_contract: soroban_sdk::Bytes,
        label: soroban_sdk::Symbol,
    ) -> Result<u32, Error> {
        caller.require_auth();
        if caller != Self::get_admin(&env) {
            return Err(Error::Unauthorized);
        }

        let counter: u32 = env
            .storage()
            .instance()
            .get(&DataKey::CrossChainEndpointCounter)
            .unwrap_or(0);
        let id = counter + 1;
        env.storage()
            .instance()
            .set(&DataKey::CrossChainEndpointCounter, &id);

        let endpoint = BridgeEndpoint {
            id,
            chain,
            destination_contract,
            label,
            enabled: true,
        };

        let mut endpoints: Vec<BridgeEndpoint> = env
            .storage()
            .instance()
            .get(&DataKey::CrossChainEndpoints)
            .unwrap_or(Vec::new(&env));
        endpoints.push_back(endpoint);
        env.storage()
            .instance()
            .set(&DataKey::CrossChainEndpoints, &endpoints);

        env.events()
            .publish((symbol_short!("XChain"), symbol_short!("endp_add")), id);

        Ok(id)
    }

    /// Remove a cross-chain bridge endpoint. Admin-only.
    pub fn remove_cross_chain_endpoint(
        env: Env,
        caller: Address,
        endpoint_id: u32,
    ) -> Result<(), Error> {
        caller.require_auth();
        if caller != Self::get_admin(&env) {
            return Err(Error::Unauthorized);
        }

        let endpoints: Vec<BridgeEndpoint> = env
            .storage()
            .instance()
            .get(&DataKey::CrossChainEndpoints)
            .unwrap_or(Vec::new(&env));

        let mut new_endpoints = Vec::new(&env);
        let mut found = false;
        for i in 0..endpoints.len() {
            let ep = endpoints.get(i).unwrap();
            if ep.id == endpoint_id {
                found = true;
            } else {
                new_endpoints.push_back(ep);
            }
        }

        if !found {
            return Err(Error::StrategyNotFound); // closest existing error
        }

        env.storage()
            .instance()
            .set(&DataKey::CrossChainEndpoints, &new_endpoints);

        env.events().publish(
            (symbol_short!("XChain"), symbol_short!("endp_rem")),
            endpoint_id,
        );

        Ok(())
    }

    /// Get all registered cross-chain bridge endpoints.
    pub fn get_cross_chain_endpoints(env: Env) -> Vec<BridgeEndpoint> {
        env.storage()
            .instance()
            .get(&DataKey::CrossChainEndpoints)
            .unwrap_or(Vec::new(&env))
    }

    /// Emit a `CrossChainRebalance` payload event.
    ///
    /// Records the payload in persistent storage under a nonce key and
    /// publishes a Soroban event for off-chain relayers to pick up.
    /// The relayer is responsible for delivering the payload to the
    /// target bridge and destination chain.
    ///
    /// # Arguments
    /// * `caller` — Must be the admin or oracle.
    /// * `destination_chain` — The target chain ID.
    /// * `destination_contract` — The contract/account on the target chain.
    /// * `asset` — The token address to rebalance.
    /// * `amount` — The amount to transfer (base units).
    /// * `memo` — Optional reference for the target chain.
    ///
    /// # Returns
    /// The nonce of this payload (for tracking/dedup).
    pub fn emit_cross_chain_rebalance(
        env: Env,
        caller: Address,
        destination_chain: u64,
        destination_contract: soroban_sdk::Bytes,
        asset: Address,
        amount: i128,
        memo: soroban_sdk::Bytes,
    ) -> Result<u64, Error> {
        caller.require_auth();
        let admin = Self::get_admin(&env);
        let oracle = Self::get_oracle(&env);
        if caller != admin && caller != oracle {
            return Err(Error::Unauthorized);
        }
        if amount <= 0 {
            return Err(Error::NegativeAmount);
        }

        let nonce: u64 = env
            .storage()
            .instance()
            .get(&DataKey::CrossChainRebalanceNonce)
            .unwrap_or(0);
        let next_nonce = nonce + 1;
        env.storage()
            .instance()
            .set(&DataKey::CrossChainRebalanceNonce, &next_nonce);

        let payload = CrossChainRebalancePayload {
            nonce: next_nonce,
            source: env.current_contract_address(),
            destination_chain,
            destination_contract,
            asset: asset.clone(),
            amount,
            timestamp: env.ledger().timestamp(),
            memo,
        };

        // Store payload so off-chain relayer can query
        env.storage()
            .persistent()
            .set(&DataKey::CrossChainRebalanceNonce, &payload);

        // Emit cross-chain rebalance event
        env.events().publish(
            (symbol_short!("XChain"), symbol_short!("rebalance")),
            (next_nonce, asset, amount, destination_chain),
        );

        Ok(next_nonce)
    }

    // ── Governance Token (SC-29) ──────────────

    /// Set the governance token address. Admin-only.
    pub fn set_governance_token(env: Env, caller: Address, token: Address) {
        caller.require_auth();
        if caller != Self::get_admin(&env) {
            panic!("Unauthorized");
        }
        env.storage().instance().set(&DataKey::GovernanceToken, &token);
        env.events().publish((symbol_short!("GovToken"), symbol_short!("set")), token);
    }

    /// Read the governance token address if set.
    pub fn get_governance_token(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::GovernanceToken)
    }

    /// Calculate the voting power of a user based on their proportional asset backing.
    pub fn get_voting_power(env: Env, user: Address) -> i128 {
        let user_shares = Self::balance(env.clone(), user);
        if user_shares == 0 {
            return 0;
        }

        let total_shares = Self::total_shares(&env);
        let total_assets = Self::total_assets(&env);

        if total_shares == 0 {
            return 0;
        }

        (user_shares * total_assets) / total_shares
    }

    /// Cast a vote. Currently unimplemented.
    pub fn cast_vote(env: Env, voter: Address, _proposal_id: u32, _support: bool) {
        voter.require_auth();
        panic_with_error!(&env, Error::NotImplemented);
    }

    // ── Contract Upgrade ──────────────────────
    /// Upgrades the contract's WebAssembly code to a new version. Admin-only.
    pub fn upgrade(env: Env, caller: Address, new_wasm_hash: soroban_sdk::BytesN<32>) {
        caller.require_auth();
        let admin = Self::get_admin(&env);
        if caller != admin {
            panic!("Unauthorized");
        }
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }
}

#[cfg(test)]
mod test;
