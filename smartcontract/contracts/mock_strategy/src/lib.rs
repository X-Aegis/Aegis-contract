#![no_std]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env,
};

// Import the StrategyTrait from volatility_shield
use soroban_sdk::contractclient;

#[contractclient(name = "StrategyTraitClient")]
pub trait StrategyTrait {
    /// Deposit assets into the strategy
    fn deposit(env: Env, amount: i128);

    /// Withdraw assets from the strategy
    fn withdraw(env: Env, amount: i128);

    /// Get the current balance of the strategy
    fn balance(env: Env) -> i128;
}

// ─────────────────────────────────────────────
// Error types
// ─────────────────────────────────────────────
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    Unauthorized = 2,
    InsufficientBalance = 3,
    NegativeAmount = 4,
}

// ─────────────────────────────────────────────
// Storage keys
// ─────────────────────────────────────────────
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Token,
    Balance,
}

// ─────────────────────────────────────────────
// Contract
// ─────────────────────────────────────────────
#[contract]
pub struct MockStrategy;

#[contractimpl]
impl MockStrategy {
    // ── Initialization ────────────────────────
    pub fn init(env: Env, admin: Address, token: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage().instance().set(&DataKey::Balance, &0i128);
    }

    // ── Core Strategy Functions ───────────────
    /// Deposit assets into the strategy
    pub fn deposit(env: Env, amount: i128) {
        if amount <= 0 {
            panic!("deposit amount must be positive");
        }
        
        // Update internal balance
        let current_balance = Self::balance(env.clone());
        env.storage().instance().set(&DataKey::Balance, &(current_balance + amount));
    }

    /// Withdraw assets from the strategy
    pub fn withdraw(env: Env, amount: i128) {
        if amount <= 0 {
            panic!("withdraw amount must be positive");
        }

        let current_balance = Self::balance(env.clone());
        if current_balance < amount {
            panic!("insufficient balance");
        }
        
        // Update internal balance first
        env.storage().instance().set(&DataKey::Balance, &(current_balance - amount));
        
        // For rebalancing, the VolatilityShield contract will handle the actual token transfer
        // We just need to update our internal balance
    }

    /// Get the current balance of the strategy
    pub fn balance(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::Balance)
            .unwrap_or(0)
    }

    // ── Admin Functions ───────────────────────
    /// Set balance directly (for testing purposes)
    pub fn set_balance(env: Env, amount: i128) {
        let admin = Self::get_admin(&env);
        admin.require_auth();
        
        if amount < 0 {
            panic!("balance cannot be negative");
        }
        
        env.storage().instance().set(&DataKey::Balance, &amount);
    }

    // ── View Functions ────────────────────────
    pub fn get_admin(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized")
    }

    pub fn get_token(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Token)
            .expect("Not initialized")
    }
}

mod test;
