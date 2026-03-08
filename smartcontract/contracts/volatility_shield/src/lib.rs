#![no_std]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, Map,
    TryFromVal, Vec,
};

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
    Requirement,
    Proposal(u64),
    Signatures(u64),
    NextProposalId,
    MaxDepositPerUser,
    MaxTotalAssets,
    MaxWithdrawPerTx,
    UserDeposited(Address),
    TimelockDuration,
    TimelockProposal,
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

    /// Set up multisig guardians and threshold.
    pub fn init_multisig(env: Env, guardians: Vec<Address>, requirement: u32) {
        let admin = Self::get_admin(&env);
        admin.require_auth();

        if guardians.len() < requirement as u32 {
            panic!("Guardians count must be >= requirement");
        }

        env.storage().instance().set(&DataKey::Guardians, &guardians);
        env.storage()
            .instance()
            .set(&DataKey::Requirement, &requirement);
    }

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

        let requirement = Self::get_requirement(&env);
        if signatures.len() >= requirement {
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

        let requirement = Self::get_requirement(&env);
        if requirement > 0 {
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
    pub fn deposit(env: Env, from: Address, amount: i128) {
        Self::assert_not_paused(&env);

        if amount <= 0 {
            panic!("deposit amount must be positive");
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

        env.events()
            .publish((symbol_short!("Deposit"), from.clone()), amount);
    }

    // ── Withdraw ──────────────────────────────
    pub fn withdraw(env: Env, from: Address, shares: i128) {
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

    // ── Rebalance ─────────────────────────────
    /// Move funds between strategies according to `allocations`.
    /// Accepts slippage tolerance in basis points (1 bps = 0.01%).
    pub fn rebalance(env: Env, allocations: Map<Address, i128>, max_slippage_bps: u32) {
        let admin = Self::get_admin(&env);
        let oracle = Self::get_oracle(&env);

        Self::require_admin_or_oracle(&env, &admin, &oracle);

        let requirement = Self::get_requirement(&env);
        if requirement > 0 {
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
    pub fn add_strategy(env: Env, strategy: Address) -> Result<(), Error> {
        let admin = Self::get_admin(&env);
        admin.require_auth();

        let requirement = Self::get_requirement(&env);
        if requirement > 0 {
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

    pub fn get_requirement(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::Requirement)
            .unwrap_or(0)
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
        let fee_pct = Self::fee_percentage(&env);
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
}

mod test;
