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

    pub fn propose_action(
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

    pub fn approve_action(env: Env, guardian: Address, proposal_id: u64) {
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
            Self::execute_proposal(&env, &mut proposal);
        }

        env.events()
            .publish((symbol_short!("Approve"), guardian, proposal_id), proposal_id);
    }

    fn execute_proposal(env: &Env, proposal: &mut Proposal) {
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

    // ── Deposit ───────────────────────────────
    pub fn deposit(env: Env, from: Address, amount: i128) {
        Self::assert_not_paused(&env);

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
        Self::set_total_shares(
            env.clone(),
            total_shares.checked_add(shares_to_mint).unwrap(),
        );
        Self::set_total_assets(env.clone(), total_assets.checked_add(amount).unwrap());

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
    pub fn rebalance(env: Env, allocations: Map<Address, i128>) {
        let admin = Self::get_admin(&env);
        let oracle = Self::get_oracle(&env);

        Self::require_admin_or_oracle(&env, &admin, &oracle);

        let requirement = Self::get_requirement(&env);
        if requirement > 0 {
            panic!("rebalance must go through multisig proposal");
        }

        Self::rebalance_internal(env, allocations);
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
        amount
            .checked_mul(total_shares)
            .unwrap()
            .checked_div(total_assets)
            .unwrap()
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
        shares
            .checked_mul(total_assets)
            .unwrap()
            .checked_div(total_shares)
            .unwrap()
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
