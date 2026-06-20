#![no_std]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, BytesN, Env,
    Map, TryFromVal, Vec,
};

/// Schema version written to storage at initialisation. Bump this constant
/// when a breaking storage migration is required and add a corresponding arm
/// in `migrate`.
pub const CURRENT_VERSION: u32 = 1;

// ─────────────────────────────────────────────
// Error types
// ─────────────────────────────────────────────
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    NegativeAmount = 3,
    Unauthorized = 4,
    NoStrategies = 5,
    DepositCapExceeded = 6,
    GlobalCapExceeded = 7,
    WithdrawCapExceeded = 8,
    TimelockNotElapsed = 9,
    TimelockNotSet = 10,
    SlippageExceeded = 11,
    /// The on-chain schema version is older than the running code requires.
    /// Call `migrate` to bring the contract up to the current version before
    /// using any state-mutating entry points.
    MigrationRequired = 12,
    /// `migrate` was called with a version that does not follow sequentially
    /// from the current stored version.
    InvalidMigrationVersion = 13,
}

// ─────────────────────────────────────────────
// Storage keys
// ─────────────────────────────────────────────
#[contracttype]
#[derive(Clone)]
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
    Paused,
    Guardians,
    Threshold,
    Proposal(u64),
    Signatures(u64),
    NextProposalId,
    MaxDepositPerUser,
    MaxTotalAssets,
    MaxWithdrawPerTx,
    UserDeposited(Address),
    TimelockDuration,
    TimelockProposal,
    /// Schema version. Written to 1 by `init` and bumped by `migrate`.
    Version,
    /// Maximum number of strategies allowed (introduced in v2). 0 = uncapped.
    MaxStrategies,
    AcceptedAssets,
    AssetBalance(Address),
    /// Withdraw request queue: WithdrawRequest(id) stores individual requests
    WithdrawRequest(u64),
    /// Counter for next withdraw request ID
    NextWithdrawRequestId,
    /// Set of pending withdraw request IDs (for enumeration)
    WithdrawQueueIds,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionType {
    SetPaused = 1,
    AddStrategy = 2,
    Rebalance = 3,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Proposal {
    pub id: u64,
    pub action_type: ActionType,
    pub description: soroban_sdk::String,
    pub creator: Address,
    pub expiration: u64,
    pub executed: bool,
    pub data: Vec<soroban_sdk::Val>, // Packed parameters for the action
}

/// Represents a queued withdrawal request.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WithdrawRequest {
    pub id: u64,
    pub user: Address,
    pub shares: i128,
    pub requested_at: u64,
    pub processed: bool,
}

// ─────────────────────────────────────────────
// Strategy cross-contract client
// ─────────────────────────────────────────────
pub struct StrategyClient<'a> {
    env: &'a Env,
    address: Address,
}

impl<'a> StrategyClient<'a> {
    pub fn new(env: &'a Env, address: Address) -> Self {
        Self { env, address }
    }

    pub fn deposit(&self, amount: i128) {
        self.env.invoke_contract::<()>(
            &self.address,
            &soroban_sdk::Symbol::new(self.env, "deposit"),
            soroban_sdk::vec![self.env, soroban_sdk::IntoVal::into_val(&amount, self.env)],
        );
    }

    pub fn withdraw(&self, amount: i128) {
        self.env.invoke_contract::<()>(
            &self.address,
            &soroban_sdk::Symbol::new(self.env, "withdraw"),
            soroban_sdk::vec![self.env, soroban_sdk::IntoVal::into_val(&amount, self.env)],
        );
    }

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
pub struct VolatilityShield;

#[contractimpl]
impl VolatilityShield {
    // ── Initialization ────────────────────────
    /// Initializes the vault contract with core roles and configuration.
    ///
    /// # Arguments
    /// * `admin` - The admin address with control over deposits, withdrawals, and contract upgrades.
    /// * `asset` - The primary stablecoin token address for deposits/withdrawals.
    /// * `oracle` - The oracle address authorized to trigger rebalancing.
    /// * `treasury` - The address that receives withdrawal fees.
    /// * `fee_percentage` - Withdrawal fee in basis points (e.g., 250 = 2.5%).
    ///
    /// # Panics
    /// If called more than once (AlreadyInitialized error).
    ///
    /// # Storage
    /// Initializes instance storage with admin, asset, oracle, treasury, strategies vector,
    /// fee percentage, accepted assets list (starting with primary asset), and version (1).
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
        env.storage()
            .instance()
            .set(&DataKey::Version, &CURRENT_VERSION);

        let mut accepted_assets = Vec::<Address>::new(&env);
        accepted_assets.push_back(asset);
        env.storage()
            .instance()
            .set(&DataKey::AcceptedAssets, &accepted_assets);
    }

    /// Add a stablecoin asset to the vault whitelist. Admin only.
    pub fn add_accepted_asset(env: Env, asset: Address) {
        let admin = Self::get_admin(&env);
        admin.require_auth();

        let mut accepted = Self::get_accepted_assets(env.clone());
        if accepted.contains(asset.clone()) {
            return;
        }
        accepted.push_back(asset.clone());
        env.storage()
            .instance()
            .set(&DataKey::AcceptedAssets, &accepted);

        env.events()
            .publish((symbol_short!("Asset"), symbol_short!("added")), asset);
    }

    // ── Upgrade & Migration ───────────────────
    /// Replaces the contract code (WASM) while preserving all storage state.
    ///
    /// # Arguments
    /// * `new_wasm_hash` - SHA-256 hash of the new contract WASM to install.
    ///
    /// # Effects
    /// - Updates the running contract code.
    /// - All storage (admin, assets, strategies, balances, etc.) is preserved.
    /// - After upgrade, call `migrate(version + 1)` if the new code requires schema updates.
    /// - Emits Upgrade event.
    ///
    /// # Important
    /// This is a CRITICAL operation. New WASM must be thoroughly tested before deployment.
    /// Incorrect WASM can permanently freeze the contract or corrupt state.
    ///
    /// # Authorization
    /// Requires `admin.require_auth()` — only admin can upgrade.
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        let admin = Self::get_admin(&env);
        admin.require_auth();
        env.deployer().update_current_contract_wasm(new_wasm_hash);
        env.events().publish(
            (symbol_short!("Upgrade"), symbol_short!("wasm")),
            env.current_contract_address(),
        );
    }

    /// Advances the storage schema from its current version to the next sequential version.
    ///
    /// # Arguments
    /// * `new_version` - Target schema version (must equal current_version + 1).
    ///
    /// # Effects
    /// - Performs data transformations required by the new version:
    ///   - v1→v2: Initializes `MaxStrategies` cap (0 = uncapped) and backfills `AcceptedAssets` list.
    /// - Updates `Version` key to new_version on success.
    /// - Emits Migrate event with (old_version, new_version).
    ///
    /// # Errors
    /// - Returns InvalidMigrationVersion if new_version != current_version + 1.
    ///
    /// # Important
    /// Migrations are strictly sequential and cannot be skipped. If current version is v1 and
    /// you want v3, call migrate(2) then migrate(3). Each migration must complete successfully.
    ///
    /// # Authorization
    /// Requires `admin.require_auth()` — only admin can perform migrations.
    pub fn migrate(env: Env, new_version: u32) -> Result<(), Error> {
        let admin = Self::get_admin(&env);
        admin.require_auth();

        let current: u32 = env
            .storage()
            .instance()
            .get(&DataKey::Version)
            .unwrap_or(1);

        if new_version != current + 1 {
            return Err(Error::InvalidMigrationVersion);
        }

        match new_version {
            2 => {
                // v1 → v2: introduce MaxStrategies cap with backward-compatible default (0 = uncapped).
                if !env.storage().instance().has(&DataKey::MaxStrategies) {
                    env.storage()
                        .instance()
                        .set(&DataKey::MaxStrategies, &0u32);
                }
                if !env.storage().instance().has(&DataKey::AcceptedAssets) {
                    let asset = Self::get_asset(&env);
                    let mut accepted_assets = Vec::<Address>::new(&env);
                    accepted_assets.push_back(asset);
                    env.storage()
                        .instance()
                        .set(&DataKey::AcceptedAssets, &accepted_assets);
                }
            }
            _ => {
                return Err(Error::InvalidMigrationVersion);
            }
        }

        env.storage()
            .instance()
            .set(&DataKey::Version, &new_version);

        env.events().publish(
            (symbol_short!("Migrate"), symbol_short!("version")),
            (current, new_version),
        );

        Ok(())
    }

    /// Returns the current on-chain schema version.
    pub fn get_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::Version)
            .unwrap_or(1)
    }

    /// Returns the max-strategies cap (0 = uncapped). Available from v2.
    pub fn get_max_strategies(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::MaxStrategies)
            .unwrap_or(0)
    }

    /// Admin-only. Set the maximum number of strategies allowed.
    /// Requires schema v2 or higher.
    pub fn set_max_strategies(env: Env, max: u32) -> Result<(), Error> {
        Self::assert_min_version(&env, 2)?;
        let admin = Self::get_admin(&env);
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::MaxStrategies, &max);
        env.events().publish(
            (symbol_short!("MaxStrat"), symbol_short!("set")),
            max,
        );
        Ok(())
    }

    /// Configures multisig governance with guardians and approval threshold.
    ///
    /// # Arguments
    /// * `guardians` - Vector of guardian addresses authorized to propose and approve actions.
    /// * `threshold` - Number of guardian signatures required to execute proposals (must be <= guardians.len()).
    ///
    /// # Effects
    /// - Stores guardians list and threshold in instance storage.
    /// - When threshold > 0, critical functions (set_paused, add_strategy, rebalance) require multisig proposal.
    /// - Enables `propose_multisig_action` and `approve_multisig_action` entry points.
    ///
    /// # Errors
    /// - Panics if guardians.len() < threshold (invalid configuration).
    ///
    /// # Authorization
    /// Requires `admin.require_auth()` — only admin can initialize multisig.
    pub fn init_multisig(env: Env, guardians: Vec<Address>, threshold: u32) {
        let admin = Self::get_admin(&env);
        admin.require_auth();

        if guardians.len() < threshold {
            panic!("Guardians count must be >= threshold");
        }

        env.storage().instance().set(&DataKey::Guardians, &guardians);
        env.storage()
            .instance()
            .set(&DataKey::Threshold, &threshold);
    }

    /// Creates a new governance proposal requiring guardian signatures.
    ///
    /// # Arguments
    /// * `creator` - Guardian address proposing the action (must be authorized and in guardians list).
    /// * `action_type` - Type of action (SetPaused, AddStrategy, or Rebalance).
    /// * `description` - Human-readable description of the proposal.
    /// * `data` - Packed Vec<Val> containing action-specific parameters:
    ///   - SetPaused: [bool] — pause state
    ///   - AddStrategy: [Address] — strategy address
    ///   - Rebalance: [Map<Address, i128>] — allocations map
    ///
    /// # Returns
    /// Proposal ID (u64) for use in `approve_multisig_action`.
    ///
    /// # Effects
    /// - Stores proposal in persistent storage with 7-day expiration.
    /// - Increments and returns next proposal ID.
    /// - Emits Proposal event.
    ///
    /// # Errors
    /// - Panics if creator is not a guardian.
    ///
    /// # Authorization
    /// Requires `creator.require_auth()` — proposer must cryptographically sign.
    pub fn propose_multisig_action(
        env: Env,
        creator: Address,
        action_type: ActionType,
        description: soroban_sdk::String,
        data: Vec<soroban_sdk::Val>,
    ) -> u64 {
        creator.require_auth();

        // Check if creator is a guardian
        let guardians = Self::get_guardians(&env);
        if !guardians.contains(creator.clone()) {
            panic!("Only guardians can propose actions");
        }

        let id = Self::get_next_proposal_id(&env);
        let proposal = Proposal {
            id,
            action_type,
            description,
            creator: creator.clone(),
            expiration: env.ledger().timestamp() + 60 * 60 * 24 * 7, // 7 days
            executed: false,
            data,
        };

        env.storage().persistent().set(&DataKey::Proposal(id), &proposal);
        env.storage()
            .instance()
            .set(&DataKey::NextProposalId, &(id + 1));

        env.events()
            .publish((symbol_short!("Proposal"), creator, id), id);

        id
    }

    /// Votes to approve a proposal; executes when threshold signatures collected.
    ///
    /// # Arguments
    /// * `guardian` - Guardian address voting (must be authorized and in guardians list).
    /// * `proposal_id` - ID of the proposal to approve.
    ///
    /// # Effects
    /// - Records guardian's signature on the proposal.
    /// - Executes proposal action if signatures.len() >= threshold:
    ///   - SetPaused: Sets vault pause state.
    ///   - AddStrategy: Adds strategy to vault's strategy list.
    ///   - Rebalance: Rebalances funds per allocations map.
    /// - Marks proposal as executed; emits Approve and Execute events.
    ///
    /// # Errors
    /// - Panics if guardian is not in guardians list.
    /// - Panics if proposal already executed.
    /// - Panics if proposal has expired (> 7 days old).
    /// - Panics if guardian already signed this proposal.
    ///
    /// # Authorization
    /// Requires `guardian.require_auth()` — guardian must cryptographically sign.
    pub fn approve_multisig_action(env: Env, guardian: Address, proposal_id: u64) {
        guardian.require_auth();

        let guardians = Self::get_guardians(&env);
        if !guardians.contains(guardian.clone()) {
            panic!("Only guardians can approve");
        }

        let mut proposal = Self::get_proposal(&env, proposal_id);
        if proposal.executed {
            panic!("Proposal already executed");
        }
        if env.ledger().timestamp() > proposal.expiration {
            panic!("Proposal expired");
        }

        let mut signatures: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::Signatures(proposal_id))
            .unwrap_or(Vec::new(&env));

        if signatures.contains(guardian.clone()) {
            panic!("Guardian already signed");
        }

        signatures.push_back(guardian.clone());
        env.storage()
            .persistent()
            .set(&DataKey::Signatures(proposal_id), &signatures);

        let threshold = Self::get_threshold(&env);
        if signatures.len() >= threshold {
            Self::execute_multisig_proposal(&env, &mut proposal);
        }

        env.events()
            .publish((symbol_short!("Approve"), guardian, proposal_id), proposal_id);
    }

    fn execute_multisig_proposal(env: &Env, proposal: &mut Proposal) {
        match proposal.action_type {
            ActionType::SetPaused => {
                let state: bool = bool::try_from_val(env, &proposal.data.get(0).unwrap()).unwrap();
                env.storage().instance().set(&DataKey::Paused, &state);
            }
            ActionType::AddStrategy => {
                let strategy: Address = Address::try_from_val(env, &proposal.data.get(0).unwrap()).unwrap();
                let mut strategies = Self::get_strategies(env);
                if !strategies.contains(strategy.clone()) {
                    strategies.push_back(strategy.clone());
                    env.storage()
                        .instance()
                        .set(&DataKey::Strategies, &strategies);
                }
            }
            ActionType::Rebalance => {
                let allocations: Map<Address, i128> = Map::try_from_val(env, &proposal.data.get(0).unwrap()).unwrap();
                // Internal rebalance logic (calling from rebalance helper)
                Self::rebalance_internal(env.clone(), allocations);
            }
        }
        proposal.executed = true;
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal.id), proposal);
        
        env.events().publish((symbol_short!("Execute"), proposal.id), proposal.id);
    }

    fn rebalance_internal(env: Env, allocations: Map<Address, i128>) {
        let asset_addr = Self::get_asset(&env);
        let token_client = token::Client::new(&env, &asset_addr);
        let vault = env.current_contract_address();

        for (strategy_addr, target_allocation) in allocations.iter() {
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
    }

    // ── Admin Circuit Breaker ─────────────────
    pub fn set_paused(env: Env, state: bool) {
        let admin = Self::get_admin(&env);
        admin.require_auth();

        let threshold = Self::get_threshold(&env);
        if threshold > 0 {
            panic!("set_paused must go through multisig proposal");
        }

        env.storage().instance().set(&DataKey::Paused, &state);
    }

    // ── Timelock Management ────────────────────
    pub fn set_timelock_duration(env: Env, duration: u64) {
        let admin = Self::get_admin(&env);
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::TimelockDuration, &duration);
        env.events().publish(
            (symbol_short!("Timelock"), symbol_short!("duration")),
            duration,
        );
    }

    pub fn get_timelock_duration(env: &Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TimelockDuration)
            .unwrap_or(0)
    }

    pub fn propose_action(env: Env) -> u64 {
        let admin = Self::get_admin(&env);
        admin.require_auth();

        let duration = Self::get_timelock_duration(&env);
        if duration == 0 {
            panic!("timelock duration not set");
        }

        let timestamp = env.ledger().timestamp();
        env.storage()
            .instance()
            .set(&DataKey::TimelockProposal, &timestamp);

        env.events().publish(
            (symbol_short!("Timelock"), symbol_short!("started")),
            timestamp,
        );

        timestamp
    }

    pub fn execute_action(env: Env) -> Result<u64, Error> {
        let duration = Self::get_timelock_duration(&env);
        if duration == 0 {
            panic!("timelock not set");
        }

        let proposal_timestamp: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TimelockProposal)
            .unwrap_or(0);

        if proposal_timestamp == 0 {
            panic!("timelock not set");
        }

        let current_timestamp = env.ledger().timestamp();
        let elapsed = current_timestamp - proposal_timestamp;

        if elapsed < duration {
            env.events().publish(
                (symbol_short!("Timelock"), symbol_short!("rejected")),
                (proposal_timestamp, current_timestamp, elapsed, duration),
            );
            panic!("timelock not elapsed");
        }

        env.storage()
            .instance()
            .set(&DataKey::TimelockProposal, &0u64);

        env.events().publish(
            (symbol_short!("Timelock"), symbol_short!("executed")),
            current_timestamp,
        );

        Ok(current_timestamp)
    }

    pub fn get_timelock_proposal_timestamp(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TimelockProposal)
            .unwrap_or(0)
    }

    // ── Cap Management (Admin) ────────────────
    /// Set per-user and global deposit caps. Only admin can call.
    pub fn set_deposit_cap(env: Env, per_user: i128, global: i128) {
        let admin = Self::get_admin(&env);
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::MaxDepositPerUser, &per_user);
        env.storage()
            .instance()
            .set(&DataKey::MaxTotalAssets, &global);
        env.events().publish(
            (symbol_short!("CapSet"), symbol_short!("deposit")),
            (per_user, global),
        );
    }

    /// Set per-transaction withdrawal cap. Only admin can call.
    pub fn set_withdraw_cap(env: Env, max_per_tx: i128) {
        let admin = Self::get_admin(&env);
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::MaxWithdrawPerTx, &max_per_tx);
        env.events().publish(
            (symbol_short!("CapSet"), symbol_short!("withdraw")),
            max_per_tx,
        );
    }

    // ── Deposit ───────────────────────────────
    /// Deposits accepted stablecoin assets into the vault and mints vault shares.
    ///
    /// # Arguments
    /// * `from` - The user address depositing funds (must authorize via require_auth).
    /// * `asset` - The stablecoin address to deposit (must be in accepted assets whitelist).
    /// * `amount` - The quantity of assets to deposit in native decimals (must be > 0).
    ///
    /// # Effects
    /// - Transfers `amount` tokens from `from` to vault.
    /// - Mints shares: `shares = amount * total_shares / total_assets` (or `amount` if vault is empty).
    /// - Updates per-user cumulative deposit tracker.
    /// - Enforces per-user and global deposit caps if configured.
    /// - Emits Deposit event with amount.
    ///
    /// # Errors
    /// - Panics if contract is paused.
    /// - Panics if amount <= 0 (NegativeAmount).
    /// - Panics if asset not in accepted assets list.
    /// - Panics if deposit exceeds per-user cap (DepositCapExceeded).
    /// - Panics if deposit exceeds global cap (GlobalCapExceeded).
    /// - Panics if migration is required (MigrationRequired).
    ///
    /// # Authorization
    /// Requires `from.require_auth()` — user must cryptographically sign the transaction.
    pub fn deposit(env: Env, from: Address, asset: Address, amount: i128) {
        Self::assert_min_version(&env, CURRENT_VERSION).expect("migration required");
        Self::assert_not_paused(&env);

        if amount <= 0 {
            panic!("deposit amount must be positive");
        }
        if !Self::is_accepted_asset(&env, &asset) {
            panic!("asset not accepted");
        }
        from.require_auth();

        // ── Per-user deposit cap check ────────
        let user_deposited: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::UserDeposited(from.clone()))
            .unwrap_or(0);
        let new_user_total = user_deposited.checked_add(amount).unwrap();

        if let Some(max_per_user) = env
            .storage()
            .instance()
            .get::<DataKey, i128>(&DataKey::MaxDepositPerUser)
        {
            if max_per_user > 0 && new_user_total > max_per_user {
                env.events().publish(
                    (symbol_short!("CapBrch"), symbol_short!("user")),
                    (from.clone(), new_user_total, max_per_user),
                );
                panic!("deposit exceeds per-user cap");
            }
        }

        // ── Global deposit cap check ──────────
        let total_assets = Self::total_assets(&env);
        let new_total = total_assets.checked_add(amount).unwrap();

        if let Some(max_total) = env
            .storage()
            .instance()
            .get::<DataKey, i128>(&DataKey::MaxTotalAssets)
        {
            if max_total > 0 && new_total > max_total {
                env.events().publish(
                    (symbol_short!("CapBrch"), symbol_short!("global")),
                    (from.clone(), new_total, max_total),
                );
                panic!("deposit exceeds global cap");
            }
        }

        token::Client::new(&env, &asset).transfer(
            &from,
            &env.current_contract_address(),
            &amount,
        );

        let asset_balance_key = DataKey::AssetBalance(asset.clone());
        let current_asset_balance: i128 = env
            .storage()
            .instance()
            .get(&asset_balance_key)
            .unwrap_or(0);
        env.storage().instance().set(
            &asset_balance_key,
            &(current_asset_balance.checked_add(amount).unwrap()),
        );

        let shares_to_mint = Self::convert_to_shares(env.clone(), amount);

        let balance_key = DataKey::Balance(from.clone());
        let current_balance: i128 = env.storage().persistent().get(&balance_key).unwrap_or(0);
        env.storage().persistent().set(
            &balance_key,
            &(current_balance.checked_add(shares_to_mint).unwrap()),
        );

        // Track cumulative user deposits
        env.storage()
            .persistent()
            .set(&DataKey::UserDeposited(from.clone()), &new_user_total);

        let total_shares = Self::total_shares(&env);
        Self::set_total_shares(
            env.clone(),
            total_shares.checked_add(shares_to_mint).unwrap(),
        );
        Self::set_total_assets(env.clone(), new_total);

        env.events().publish(
            (symbol_short!("Deposit"), from.clone(), asset),
            amount,
        );
    }

    // ── Withdraw ──────────────────────────────
    /// Redeems vault shares and withdraws underlying assets minus fees.
    ///
    /// # Arguments
    /// * `from` - The user address redeeming shares (must authorize via require_auth).
    /// * `shares` - The quantity of vault shares to burn (must be > 0 and <= user's balance).
    ///
    /// # Effects
    /// - Burns `shares` from user's balance.
    /// - Calculates assets: `assets = shares * total_assets / total_shares`.
    /// - Deducts withdrawal fee: `net_assets = assets * (10000 - fee_pct) / 10000`.
    /// - Transfers net_assets to user; transfers fee to treasury if fee > 0.
    /// - Updates total shares and total assets.
    /// - Enforces per-transaction withdrawal cap if configured.
    /// - Emits Withdraw event with share amount.
    ///
    /// # Errors
    /// - Panics if contract is paused.
    /// - Panics if shares <= 0 (invalid amount).
    /// - Panics if shares > user's balance (InsufficientBalance).
    /// - Panics if calculated assets exceed per-tx cap (WithdrawCapExceeded).
    /// - Panics if migration is required (MigrationRequired).
    ///
    /// # Authorization
    /// Requires `from.require_auth()` — user must cryptographically sign the transaction.
    pub fn withdraw(env: Env, from: Address, shares: i128) {
        Self::assert_min_version(&env, CURRENT_VERSION).expect("migration required");
        Self::assert_not_paused(&env);

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

        // ── Per-transaction withdrawal cap check ─
        if let Some(max_withdraw) = env
            .storage()
            .instance()
            .get::<DataKey, i128>(&DataKey::MaxWithdrawPerTx)
        {
            if max_withdraw > 0 && assets_to_withdraw > max_withdraw {
                panic!("withdrawal exceeds per-transaction cap");
            }
        }
        let (net_assets, fee) = Self::take_fees(&env, assets_to_withdraw);

        let total_shares = Self::total_shares(&env);
        let total_assets = Self::total_assets(&env);

        Self::set_total_shares(env.clone(), total_shares.checked_sub(shares).unwrap());
        Self::set_total_assets(
            env.clone(),
            total_assets.checked_sub(assets_to_withdraw).unwrap(),
        );
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
            env.events()
                .publish((symbol_short!("Fee"), symbol_short!("collect")), fee);
        }

        env.events()
            .publish((symbol_short!("Withdraw"), from.clone()), shares);
    }

    // ── Withdraw Queue Management ─────────────
    /// Queues a withdrawal request for batch processing.
    ///
    /// # Arguments
    /// * `from` - User address requesting withdrawal (must authorize via require_auth).
    /// * `shares` - Amount of vault shares to withdraw (must be > 0 and <= user's balance).
    ///
    /// # Effects
    /// - Creates a pending WithdrawRequest in storage.
    /// - Shares remain in user's balance until processed.
    /// - User can cancel the request before it's processed.
    /// - Emits WithdrawQueued event with request ID.
    ///
    /// # Returns
    /// Request ID (u64) for use in `cancel_withdraw()`.
    ///
    /// # Errors
    /// - Panics if shares <= 0.
    /// - Panics if shares > user's balance.
    /// - Panics if contract is paused.
    ///
    /// # Authorization
    /// Requires `from.require_auth()` — user must cryptographically sign.
    pub fn queue_withdraw(env: Env, from: Address, shares: i128) -> u64 {
        Self::assert_not_paused(&env);

        if shares <= 0 {
            panic!("shares to withdraw must be positive");
        }
        from.require_auth();

        let balance_key = DataKey::Balance(from.clone());
        let current_balance: i128 = env.storage().persistent().get(&balance_key).unwrap_or(0);

        if current_balance < shares {
            panic!("insufficient shares for withdrawal");
        }

        let id = Self::get_next_withdraw_request_id(&env);
        let request = WithdrawRequest {
            id,
            user: from.clone(),
            shares,
            requested_at: env.ledger().timestamp(),
            processed: false,
        };

        env.storage()
            .persistent()
            .set(&DataKey::WithdrawRequest(id), &request);

        let mut queue_ids: Vec<u64> = env
            .storage()
            .instance()
            .get(&DataKey::WithdrawQueueIds)
            .unwrap_or(Vec::new(&env));
        queue_ids.push_back(id);
        env.storage()
            .instance()
            .set(&DataKey::WithdrawQueueIds, &queue_ids);

        env.storage()
            .instance()
            .set(&DataKey::NextWithdrawRequestId, &(id + 1));

        env.events().publish(
            (symbol_short!("WithdrawQ"), from.clone(), id),
            shares,
        );

        id
    }

    /// Processes all queued withdrawal requests (admin/keeper only).
    ///
    /// # Effects
    /// - Iterates through all pending WithdrawRequest entries.
    /// - For each request:
    ///   - Converts shares to assets.
    ///   - Deducts fees.
    ///   - Transfers net assets to user.
    ///   - Transfers fee to treasury.
    ///   - Marks request as processed.
    /// - Updates total shares and total assets.
    /// - Removes processed requests from queue.
    /// - Emits WithdrawProcessed event for each request.
    ///
    /// # Errors
    /// - Panics if not authorized (admin only).
    /// - Panics if contract is paused.
    ///
    /// # Authorization
    /// Requires `admin.require_auth()` — only admin can process the queue.
    pub fn process_withdraw_queue(env: Env) {
        Self::assert_not_paused(&env);
        let admin = Self::get_admin(&env);
        admin.require_auth();

        let queue_ids: Vec<u64> = env
            .storage()
            .instance()
            .get(&DataKey::WithdrawQueueIds)
            .unwrap_or(Vec::new(&env));

        if queue_ids.is_empty() {
            env.events()
                .publish((symbol_short!("QueueEmpt"),), true);
            return;
        }

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .expect("Token not initialized");
        let token_client = token::Client::new(&env, &token_addr);
        let contract_addr = env.current_contract_address();
        let treasury_addr = Self::treasury(&env);

        let mut total_shares_burned: i128 = 0;
        let mut total_assets_withdrawn: i128 = 0;
        let mut processed_ids: Vec<u64> = Vec::new(&env);

        for request_id in queue_ids.iter() {
            if let Some(mut request) = env
                .storage()
                .persistent()
                .get::<DataKey, WithdrawRequest>(&DataKey::WithdrawRequest(request_id))
            {
                if request.processed {
                    continue;
                }

                let shares = request.shares;
                let assets_to_withdraw = Self::convert_to_assets(env.clone(), shares);
                let (net_assets, fee) = Self::take_fees(&env, assets_to_withdraw);

                // Transfer net assets to user
                token_client.transfer(&contract_addr, &request.user, &net_assets);

                // Transfer fee to treasury if any
                if fee > 0 {
                    token_client.transfer(&contract_addr, &treasury_addr, &fee);
                }

                // Update user's balance
                let balance_key = DataKey::Balance(request.user.clone());
                let current_balance: i128 =
                    env.storage().persistent().get(&balance_key).unwrap_or(0);
                env.storage().persistent().set(
                    &balance_key,
                    &(current_balance.checked_sub(shares).unwrap()),
                );

                // Mark as processed
                request.processed = true;
                env.storage()
                    .persistent()
                    .set(&DataKey::WithdrawRequest(request_id), &request);

                total_shares_burned = total_shares_burned.checked_add(shares).unwrap();
                total_assets_withdrawn = total_assets_withdrawn
                    .checked_add(assets_to_withdraw)
                    .unwrap();

                processed_ids.push_back(request_id);

                env.events().publish(
                    (symbol_short!("WthdrwPrc"), request.user.clone(), request_id),
                    (shares, net_assets, fee),
                );
            }
        }

        // Update total shares and assets
        let total_shares = Self::total_shares(&env);
        let total_assets = Self::total_assets(&env);
        Self::set_total_shares(
            env.clone(),
            total_shares.checked_sub(total_shares_burned).unwrap(),
        );
        Self::set_total_assets(
            env.clone(),
            total_assets.checked_sub(total_assets_withdrawn).unwrap(),
        );

        // Remove processed requests from queue
        let mut new_queue: Vec<u64> = Vec::new(&env);
        for id in queue_ids.iter() {
            if !processed_ids.contains(id.clone()) {
                new_queue.push_back(id);
            }
        }
        env.storage()
            .instance()
            .set(&DataKey::WithdrawQueueIds, &new_queue);

        env.events().publish(
            (symbol_short!("QueueProc"),),
            (processed_ids.len(), total_shares_burned, total_assets_withdrawn),
        );
    }

    /// Cancels a pending withdrawal request.
    ///
    /// # Arguments
    /// * `from` - User address canceling the request (must authorize via require_auth).
    /// * `request_id` - ID of the WithdrawRequest to cancel.
    ///
    /// # Effects
    /// - Removes the WithdrawRequest from storage.
    /// - Removes request ID from the queue.
    /// - Shares remain in user's balance (not withdrawn).
    /// - Emits WithdrawCancelled event.
    ///
    /// # Errors
    /// - Panics if request not found.
    /// - Panics if request already processed.
    /// - Panics if user is not the request creator.
    ///
    /// # Authorization
    /// Requires `from.require_auth()` — user must cryptographically sign.
    pub fn cancel_withdraw(env: Env, from: Address, request_id: u64) {
        from.require_auth();

        let request = Self::get_withdraw_request(&env, request_id);

        if request.user != from {
            panic!("Only request creator can cancel");
        }

        if request.processed {
            panic!("Cannot cancel processed request");
        }

        // Remove from storage
        env.storage().persistent().remove(&DataKey::WithdrawRequest(request_id));

        // Remove from queue
        let queue_ids: Vec<u64> = env
            .storage()
            .instance()
            .get(&DataKey::WithdrawQueueIds)
            .unwrap_or(Vec::new(&env));

        let mut new_queue: Vec<u64> = Vec::new(&env);
        for id in queue_ids.iter() {
            if id != request_id {
                new_queue.push_back(id);
            }
        }
        env.storage()
            .instance()
            .set(&DataKey::WithdrawQueueIds, &new_queue);

        env.events().publish(
            (symbol_short!("WthdrwCan"), from.clone(), request_id),
            request.shares,
        );
    }

    // ── Rebalance ─────────────────────────────
    /// Reallocates funds between yield strategies to match target allocations.
    ///
    /// # Arguments
    /// * `allocations` - Map of strategy addresses to target asset amounts.
    /// * `max_slippage_bps` - Maximum acceptable slippage in basis points (100 = 1%).
    ///
    /// # Effects
    /// - Deposits or withdraws from each strategy to match target allocations.
    /// - Checks actual post-rebalance balance against expected; panics if slippage exceeds limit.
    /// - Updates internal strategy balances via cross-contract calls.
    /// - Emits Slippage event if tolerance exceeded (before panicking).
    ///
    /// # Errors
    /// - Panics if not authorized (admin or oracle).
    /// - Panics if multisig governance enabled (threshold > 0); must use multisig proposal path.
    /// - Panics if slippage exceeds max_slippage_bps for any strategy (SlippageExceeded).
    /// - Panics if migration is required (MigrationRequired).
    ///
    /// # Authorization
    /// Requires `admin.require_auth()` or `oracle.require_auth()` — only admin or oracle can rebalance.
    /// If multisig is enabled (threshold > 0), this function rejects; use `propose_multisig_action` instead.
    ///
    /// # Note
    /// Slippage is computed post-transaction. If strategies mutate balances unexpectedly
    /// (e.g., rounding or yield accumulation), slippage may not reflect the full impact.
    pub fn rebalance(env: Env, allocations: Map<Address, i128>, max_slippage_bps: u32) {
        Self::assert_min_version(&env, CURRENT_VERSION).expect("migration required");
        let admin = Self::get_admin(&env);
        let oracle = Self::get_oracle(&env);

        Self::require_admin_or_oracle(&env, &admin, &oracle);

        let threshold = Self::get_threshold(&env);
        if threshold > 0 {
            panic!("rebalance must go through multisig proposal");
        }

        let asset_addr = Self::get_asset(&env);
        let token_client = token::Client::new(&env, &asset_addr);
        let vault = env.current_contract_address();

        for (strategy_addr, target_allocation) in allocations.iter() {
            let strategy = StrategyClient::new(&env, strategy_addr.clone());
            let current_balance = strategy.balance();

            let delta = Self::calc_rebalance_delta(env.clone(), current_balance, target_allocation);
            let expected_balance = target_allocation;

            if delta > 0 {
                token_client.transfer(&vault, &strategy_addr, &delta);
                strategy.deposit(delta);
            } else if delta < 0 {
                let amount_to_withdraw = delta.abs();
                strategy.withdraw(amount_to_withdraw);
                token_client.transfer(&strategy_addr, &vault, &amount_to_withdraw);
            }

            let actual_balance = strategy.balance();
            if max_slippage_bps > 0 {
                Self::check_slippage(
                    &env,
                    expected_balance,
                    actual_balance,
                    max_slippage_bps,
                    strategy_addr.clone(),
                );
            }
        }

        Self::rebalance_internal(env, allocations);
    }

    fn check_slippage(
        env: &Env,
        expected: i128,
        actual: i128,
        max_slippage_bps: u32,
        strategy_addr: Address,
    ) {
        if expected == 0 {
            return;
        }
        let diff = (expected - actual).abs();
        let slippage_bps: u32 = (diff
            .checked_mul(10000)
            .unwrap()
            .checked_div(expected)
            .unwrap()) as u32;

        if slippage_bps > max_slippage_bps {
            env.events().publish(
                (symbol_short!("Slippage"), symbol_short!("exceeded")),
                (
                    strategy_addr,
                    expected,
                    actual,
                    slippage_bps,
                    max_slippage_bps,
                ),
            );
            panic!("slippage exceeded");
        }
    }

    /// Calculate the exact delta needed to reach the target allocation.
    /// Returns a positive number if funds need to be added (deposit).
    /// Returns a negative number if funds need to be removed (withdraw).
    /// Returns 0 if no change is needed.
    pub fn calc_rebalance_delta(_env: Env, current: i128, target: i128) -> i128 {
        if target < 0 || current < 0 {
            panic!("Balances cannot be negative");
        }

        target
            .checked_sub(current)
            .expect("Delta calculation overflow")
    }

    // ── Strategy Management ───────────────────
    /// Registers a new yield strategy contract with the vault.
    ///
    /// # Arguments
    /// * `strategy` - Address of the strategy contract to add.
    ///
    /// # Effects
    /// - Adds strategy to vault's strategy list (if not already present).
    /// - Strategy must implement deposit(amount), withdraw(amount), and balance() callables.
    /// - Emits Strategy event.
    ///
    /// # Errors
    /// - Returns AlreadyInitialized if strategy is already registered.
    /// - Panics if not authorized (admin only; or multisig proposal if threshold > 0).
    /// - Panics if multisig governance enabled; must use `propose_multisig_action` instead.
    ///
    /// # Authorization
    /// Requires `admin.require_auth()` — only admin can add strategies.
    /// If multisig is enabled (threshold > 0), this function rejects; use multisig proposal path.
    pub fn add_strategy(env: Env, strategy: Address) -> Result<(), Error> {
        let admin = Self::get_admin(&env);
        admin.require_auth();

        let threshold = Self::get_threshold(&env);
        if threshold > 0 {
            panic!("add_strategy must go through multisig proposal");
        }

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

    /// Collects accrued yield from all registered strategies and updates vault accounting.
    ///
    /// # Effects
    /// - Calls `balance()` on each strategy to query accrued yield.
    /// - Sums total yield across all strategies.
    /// - Increments `total_assets` by total_yield if yield > 0.
    /// - Does NOT transfer tokens (assumes yield is already in vault or strategies).
    /// - Emits harvest event with total_yield.
    ///
    /// # Returns
    /// Total yield amount collected (i128).
    ///
    /// # Errors
    /// - Returns NoStrategies if no strategies are registered.
    ///
    /// # Authorization
    /// Requires `admin.require_auth()` — only admin can harvest.
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

        env.events()
            .publish((symbol_short!("harvest"),), total_yield);
        Ok(total_yield)
    }

    // ── View helpers ──────────────────────────
    pub fn total_assets(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalAssets)
            .unwrap_or(0)
    }

    pub fn total_shares(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalShares)
            .unwrap_or(0)
    }

    pub fn get_admin(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized")
    }

    pub fn get_oracle(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Oracle)
            .expect("Not initialized")
    }

    pub fn get_asset(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Asset)
            .expect("Not initialized")
    }

    pub fn get_strategies(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::Strategies)
            .unwrap_or(Vec::new(env))
    }

    pub fn treasury(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Treasury)
            .expect("Not initialized")
    }

    pub fn fee_percentage(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::FeePercentage)
            .unwrap_or(0)
    }

    pub fn balance(env: Env, user: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Balance(user))
            .unwrap_or(0)
    }

    pub fn get_guardians(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::Guardians)
            .unwrap_or(Vec::new(env))
    }

    pub fn get_threshold(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::Threshold)
            .unwrap_or(0)
    }

    // ── Guardian Management (Admin Only) ──────
    pub fn add_guardian(env: Env, guardian: Address) {
        let admin = Self::get_admin(&env);
        admin.require_auth();

        let mut guardians = Self::get_guardians(&env);
        if guardians.contains(guardian.clone()) {
            panic!("Guardian already exists");
        }

        guardians.push_back(guardian.clone());
        env.storage().instance().set(&DataKey::Guardians, &guardians);

        env.events().publish(
            (symbol_short!("Guardian"), symbol_short!("added")),
            guardian,
        );
    }

    pub fn remove_guardian(env: Env, guardian: Address) {
        let admin = Self::get_admin(&env);
        admin.require_auth();

        let mut guardians = Self::get_guardians(&env);
        if let Some(index) = guardians.first_index_of(guardian.clone()) {
            guardians.remove(index);

            // Check threshold validity
            let threshold = Self::get_threshold(&env);
            if guardians.len() < threshold {
                panic!("Cannot remove guardian: would break threshold");
            }

            env.storage().instance().set(&DataKey::Guardians, &guardians);

            env.events().publish(
                (symbol_short!("Guardian"), symbol_short!("removed")),
                guardian,
            );
        } else {
            panic!("Guardian not found");
        }
    }

    pub fn set_threshold(env: Env, threshold: u32) {
        let admin = Self::get_admin(&env);
        admin.require_auth();

        let guardians = Self::get_guardians(&env);
        if guardians.len() < threshold {
            panic!("Threshold cannot be greater than guardians count");
        }

        if threshold == 0 {
            panic!("Threshold must be at least 1");
        }

        env.storage().instance().set(&DataKey::Threshold, &threshold);

        env.events().publish(
            (symbol_short!("Guardian"), symbol_short!("thr_set")),
            threshold,
        );
    }

    pub fn get_proposal(env: &Env, id: u64) -> Proposal {
        env.storage()
            .persistent()
            .get(&DataKey::Proposal(id))
            .expect("Proposal not found")
    }

    pub fn get_next_proposal_id(env: &Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::NextProposalId)
            .unwrap_or(0)
    }

    /// Returns (per_user_cap, global_cap). Returns (0, 0) if not set.
    pub fn get_deposit_cap(env: Env) -> (i128, i128) {
        let per_user: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MaxDepositPerUser)
            .unwrap_or(0);
        let global: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MaxTotalAssets)
            .unwrap_or(0);
        (per_user, global)
    }

    /// Returns the per-transaction withdrawal cap. Returns 0 if not set.
    pub fn get_withdraw_cap(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::MaxWithdrawPerTx)
            .unwrap_or(0)
    }

    /// Returns total amount deposited by a user (cumulative).
    pub fn get_user_deposited(env: Env, user: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::UserDeposited(user))
            .unwrap_or(0)
    }

    /// Returns the whitelisted stablecoin assets accepted by the vault.
    pub fn get_accepted_assets(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::AcceptedAssets)
            .unwrap_or(Vec::new(&env))
    }

    /// Returns the vault's balance for a specific accepted asset.
    pub fn get_asset_balance(env: Env, asset: Address) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::AssetBalance(asset))
            .unwrap_or(0)
    }

    /// Retrieves a specific withdraw request by ID.
    pub fn get_withdraw_request(env: &Env, request_id: u64) -> WithdrawRequest {
        env.storage()
            .persistent()
            .get(&DataKey::WithdrawRequest(request_id))
            .expect("Withdraw request not found")
    }

    /// Returns the next withdraw request ID to be assigned.
    pub fn get_next_withdraw_request_id(env: &Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::NextWithdrawRequestId)
            .unwrap_or(0)
    }

    /// Returns all pending withdraw request IDs in the queue.
    pub fn get_withdraw_queue(env: Env) -> Vec<u64> {
        env.storage()
            .instance()
            .get(&DataKey::WithdrawQueueIds)
            .unwrap_or(Vec::new(&env))
    }

    /// Returns all pending withdraw requests for a specific user.
    pub fn get_user_pending_withdrawals(env: Env, user: Address) -> Vec<WithdrawRequest> {
        let queue_ids: Vec<u64> = env
            .storage()
            .instance()
            .get(&DataKey::WithdrawQueueIds)
            .unwrap_or(Vec::new(&env));

        let mut user_requests = Vec::new(&env);
        for request_id in queue_ids.iter() {
            if let Some(request) = env
                .storage()
                .persistent()
                .get::<DataKey, WithdrawRequest>(&DataKey::WithdrawRequest(request_id))
            {
                if request.user == user && !request.processed {
                    user_requests.push_back(request);
                }
            }
        }
        user_requests
    }

    // ── Internal Helpers ──────────────────────
    fn assert_not_paused(env: &Env) {
        let is_paused: bool = env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        if is_paused {
            panic!("Contract is paused");
        }
    }

    pub fn take_fees(env: &Env, amount: i128) -> (i128, i128) {
        let fee_pct = Self::fee_percentage(env);
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

    pub fn convert_to_shares(env: Env, amount: i128) -> i128 {
        if amount < 0 {
            panic!("negative amount");
        }
        let total_shares = Self::total_shares(&env);
        let total_assets = Self::total_assets(&env);
        if total_shares == 0 || total_assets == 0 {
            return amount;
        }

        // Use I256 to prevent overflow during (amount * total_shares)
        let amount_256 = soroban_sdk::I256::from_i128(&env, amount);
        let total_shares_256 = soroban_sdk::I256::from_i128(&env, total_shares);
        let total_assets_256 = soroban_sdk::I256::from_i128(&env, total_assets);

        let res_256 = amount_256.mul(&total_shares_256).div(&total_assets_256);
        res_256.to_i128().expect("result overflow")
    }

    pub fn convert_to_assets(env: Env, shares: i128) -> i128 {
        if shares < 0 {
            panic!("negative amount");
        }
        let total_shares = Self::total_shares(&env);
        let total_assets = Self::total_assets(&env);
        if total_shares == 0 {
            return shares;
        }

        // Use I256 to prevent overflow during (shares * total_assets)
        let shares_256 = soroban_sdk::I256::from_i128(&env, shares);
        let total_assets_256 = soroban_sdk::I256::from_i128(&env, total_assets);
        let total_shares_256 = soroban_sdk::I256::from_i128(&env, total_shares);

        let res_256 = shares_256.mul(&total_assets_256).div(&total_shares_256);
        res_256.to_i128().expect("result overflow")
    }

    pub fn set_total_assets(env: Env, amount: i128) {
        env.storage().instance().set(&DataKey::TotalAssets, &amount);
    }

    pub fn set_total_shares(env: Env, amount: i128) {
        env.storage().instance().set(&DataKey::TotalShares, &amount);
    }

    pub fn set_balance(env: Env, user: Address, amount: i128) {
        env.storage()
            .persistent()
            .set(&DataKey::Balance(user), &amount);
    }

    pub fn set_token(env: Env, token: Address) {
        env.storage().instance().set(&DataKey::Token, &token);
    }

    fn require_admin_or_oracle(env: &Env, admin: &Address, oracle: &Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            admin.require_auth();
        } else {
            oracle.require_auth();
        }
    }

    fn is_accepted_asset(env: &Env, asset: &Address) -> bool {
        Self::get_accepted_assets(env.clone()).contains(asset.clone())
    }

    /// Panics with `MigrationRequired` if the stored schema version is below
    /// `min_version`. Call this at the start of any entry point that depends on
    /// storage fields introduced after v1.
    fn assert_min_version(env: &Env, min_version: u32) -> Result<(), Error> {
        let stored: u32 = env
            .storage()
            .instance()
            .get(&DataKey::Version)
            .unwrap_or(1);
        if stored < min_version {
            return Err(Error::MigrationRequired);
        }
        Ok(())
    }
}

mod test;
