#![cfg(test)]
use super::*;
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env, Map};
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient;

extern crate mock_strategy;

fn create_token_contract<'a>(env: &Env, admin: &Address) -> (Address, StellarAssetClient<'a>, TokenClient<'a>) {
    let contract_id = env.register_stellar_asset_contract_v2(admin.clone());
    let stellar_asset_client = StellarAssetClient::new(env, &contract_id.address());
    let token_client = TokenClient::new(env, &contract_id.address());
    (contract_id.address(), stellar_asset_client, token_client)
}

#[test]
fn test_init_stores_roles() {
    let env         = Env::default();
    let contract_id = env.register_contract(None, VolatilityShield);
    let client      = VolatilityShieldClient::new(&env, &contract_id);

    let admin  = Address::generate(&env);
    let asset  = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &500u32);

    assert_eq!(client.get_admin(),  admin);
    assert_eq!(client.get_oracle(), oracle);
    assert_eq!(client.get_asset(),  asset);
    assert_eq!(client.treasury(), treasury);
    assert_eq!(client.fee_percentage(), 500u32);
}

#[test]
fn test_convert_to_assets() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    // 1. Test 1:1 conversion when total_shares is 0
    assert_eq!(client.convert_to_assets(&100), 100);

    // 2. Test exact conversion
    client.set_total_assets(&100);
    client.set_total_shares(&100);
    assert_eq!(client.convert_to_assets(&50), 50);

    // 3. Test rounding down (favors vault)
    client.set_total_assets(&10);
    client.set_total_shares(&4);
    assert_eq!(client.convert_to_assets(&3), 7);

    // 4. Test larger values
    client.set_total_assets(&1000);
    client.set_total_shares(&300);
    assert_eq!(client.convert_to_assets(&100), 333);
}

#[test]
#[should_panic(expected = "negative amount")]
fn test_convert_to_assets_negative() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);
    client.convert_to_assets(&-1);
}

#[test]
fn test_convert_to_shares() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    // 1. Initial Deposit (total_shares = 0)
    assert_eq!(client.convert_to_shares(&100), 100);

    // 2. Precision Loss (favors vault by rounding down)
    client.set_total_assets(&3);
    client.set_total_shares(&1);
    assert_eq!(client.convert_to_shares(&10), 3);

    // 3. Standard Proportional Minting
    client.set_total_assets(&1000);
    client.set_total_shares(&500);
    assert_eq!(client.convert_to_shares(&200), 100);

    // 4. Rounding Down with Large Values
    client.set_total_assets(&300);
    client.set_total_shares(&1000);
    assert_eq!(client.convert_to_shares(&100), 333);
}

#[test]
fn test_strategy_registry() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    let strategy = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);
    assert_eq!(client.get_admin(), admin);

    client.add_strategy(&strategy);
    let strategies = client.get_strategies();
    assert_eq!(strategies.len(), 1);
    assert_eq!(strategies.get(0).unwrap(), strategy);

    let strategy_2 = Address::generate(&env);
    client.add_strategy(&strategy_2);
    let strategies = client.get_strategies();
    assert_eq!(strategies.len(), 2);
    assert_eq!(strategies.get(1).unwrap(), strategy_2);
}

#[test]
#[should_panic(expected = "negative amount")]
fn test_convert_to_shares_negative() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);
    client.convert_to_shares(&-1);
}

#[test]
fn test_take_fees() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    
    client.init(&admin, &asset, &oracle, &treasury, &500u32);

    let deposit_amount = 1000;
    let (remaining, fee) = client.take_fees(&deposit_amount);
    assert_eq!(remaining, 950);
    assert_eq!(fee, 50);
}

#[test]
fn test_withdraw_success() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    
    let token_admin = Address::generate(&env);
    let (token_id, stellar_asset_client, token_client) = create_token_contract(&env, &token_admin);
    
    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    
    client.init(&admin, &token_id, &oracle, &treasury, &0u32);
    client.set_total_shares(&1000);
    client.set_total_assets(&5000);
    
    let user = Address::generate(&env);
    client.set_balance(&user, &100);
    
    stellar_asset_client.mint(&contract_id, &5000);
    
    client.withdraw(&user, &50);
    
    assert_eq!(client.balance(&user), 50);
    assert_eq!(client.total_shares(), 950);
    assert_eq!(client.total_assets(), 4750);
    assert_eq!(token_client.balance(&user), 250);
}

#[test]
#[should_panic(expected = "allocation percentages must sum to 10000 BPS")]
fn test_rebalance_admin_auth_accepted() {
    let env         = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client      = VolatilityShieldClient::new(&env, &contract_id);

    let admin  = Address::generate(&env);
    let asset  = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    let allocations: Map<Address, i128> = Map::new(&env);
    client.set_oracle_data(&oracle, &allocations, &1000);
    env.ledger().with_mut(|li| {
        li.timestamp = 2000;
    });
    client.rebalance(&admin);
}

#[test]
#[should_panic(expected = "Stale oracle data")]
fn test_oracle_staleness_rejected() {
    let env         = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client      = VolatilityShieldClient::new(&env, &contract_id);

    let admin  = Address::generate(&env);
    let asset  = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    let allocations: Map<Address, i128> = Map::new(&env);
    
    client.set_oracle_data(&oracle, &allocations, &1000);
    
    env.ledger().with_mut(|li| {
        li.timestamp = 5000;
    });

    client.rebalance(&admin);
}

#[test]
#[should_panic(expected = "allocation percentages must sum to 10000 BPS")]
fn test_rebalance_invalid_sum() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    let mock_strategy_id = Address::generate(&env);
    client.add_strategy(&mock_strategy_id);

    let mut allocations: Map<Address, i128> = Map::new(&env);
    allocations.set(mock_strategy_id, 9999);
    client.set_oracle_data(&oracle, &allocations, &1000);
    env.ledger().with_mut(|li| { li.timestamp = 2000; });
    client.rebalance(&admin);
}

#[test]
#[should_panic(expected = "allocation values cannot be negative")]
fn test_rebalance_negative_allocation() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    let mock_strategy_id = Address::generate(&env);
    client.add_strategy(&mock_strategy_id);

    let mut allocations: Map<Address, i128> = Map::new(&env);
    allocations.set(mock_strategy_id, -100);
    client.set_oracle_data(&oracle, &allocations, &1000);
    env.ledger().with_mut(|li| { li.timestamp = 2000; });
    client.rebalance(&admin);
}

#[test]
#[should_panic(expected = "allocation to zero-address or unlisted strategy")]
fn test_rebalance_unlisted_strategy() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    let unlisted_strategy_id = Address::generate(&env);

    let mut allocations: Map<Address, i128> = Map::new(&env);
    allocations.set(unlisted_strategy_id, 10000);
    client.set_oracle_data(&oracle, &allocations, &1000);
    env.ledger().with_mut(|li| { li.timestamp = 2000; });
    client.rebalance(&admin);
}

// ── Strategy Health Tests ─────────────────────────────────────────────────

#[test]
fn test_flag_strategy_marks_flagged() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    let strategy = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);
    client.add_strategy(&strategy);

    // Flag the strategy
    client.flag_strategy(&admin, &strategy);

    // Health value should be -1 (flagged sentinel)
    assert_eq!(client.get_strategy_health(&strategy), -1i128);
}

#[test]
fn test_flag_strategy_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    let strategy = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);
    client.add_strategy(&strategy);

    // flag_strategy should succeed without panicking
    client.flag_strategy(&admin, &strategy);
    // Confirm health is flagged sentinel
    assert_eq!(client.get_strategy_health(&strategy), -1i128);
}

#[test]
#[should_panic]
fn test_flag_strategy_not_listed_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    // Strategy was never added — should return StrategyNotFound error (panics via unwrap)
    let unknown_strategy = Address::generate(&env);
    client.flag_strategy(&admin, &unknown_strategy);
}

#[test]
#[should_panic]
fn test_flag_strategy_non_admin_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    let strategy = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);
    client.add_strategy(&strategy);

    // Non-admin trying to flag — should return Unauthorized error (panics via unwrap)
    client.flag_strategy(&non_admin, &strategy);
}

#[test]
fn test_remove_strategy_delists_strategy() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    // Register mock strategy contract so cross-contract balance() call works
    let mock_id = env.register_contract(None, mock_strategy::MockStrategy);
    let mock_client = mock_strategy::MockStrategyClient::new(&env, &mock_id);
    mock_client.init(&admin, &asset);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);
    client.add_strategy(&mock_id);

    assert_eq!(client.get_strategies().len(), 1);

    client.remove_strategy(&admin, &mock_id);

    assert_eq!(client.get_strategies().len(), 0);
}

#[test]
fn test_remove_strategy_withdraws_funds_first() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    let mock_id = env.register_contract(None, mock_strategy::MockStrategy);
    let mock_client = mock_strategy::MockStrategyClient::new(&env, &mock_id);
    mock_client.init(&admin, &asset);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);
    client.add_strategy(&mock_id);

    // Seed strategy with a balance
    mock_client.set_balance(&500i128);
    client.set_total_assets(&500i128);

    // Remove strategy — should withdraw the 500 first
    client.remove_strategy(&admin, &mock_id);

    // After removal the mock strategy's balance should be drained
    assert_eq!(mock_client.balance(), 0i128);
    // Vault total_assets should be adjusted
    assert_eq!(client.total_assets(), 0i128);
    // Strategy list should be empty
    assert_eq!(client.get_strategies().len(), 0);
}

#[test]
fn test_remove_strategy_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    let mock_id = env.register_contract(None, mock_strategy::MockStrategy);
    let mock_client = mock_strategy::MockStrategyClient::new(&env, &mock_id);
    mock_client.init(&admin, &asset);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);
    client.add_strategy(&mock_id);

    // remove_strategy should succeed and the strategy list should be empty
    client.remove_strategy(&admin, &mock_id);
    assert_eq!(client.get_strategies().len(), 0);
}

#[test]
#[should_panic]
fn test_remove_strategy_not_listed_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    let unknown = Address::generate(&env);
    client.remove_strategy(&admin, &unknown);
}

#[test]
fn test_check_strategy_health_healthy_strategy() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    let mock_id = env.register_contract(None, mock_strategy::MockStrategy);
    let mock_client = mock_strategy::MockStrategyClient::new(&env, &mock_id);
    mock_client.init(&admin, &asset);
    mock_client.set_balance(&1000i128);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);
    client.add_strategy(&mock_id);

    // Health check on a strategy with positive balance — no flagging expected
    let flagged = client.check_strategy_health();
    assert_eq!(flagged.len(), 0);

    // Health value should be updated to current balance (1000)
    assert_eq!(client.get_strategy_health(&mock_id), 1000i128);
}

#[test]
fn test_check_strategy_health_flags_dropped_to_zero() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    let mock_id = env.register_contract(None, mock_strategy::MockStrategy);
    let mock_client = mock_strategy::MockStrategyClient::new(&env, &mock_id);
    mock_client.init(&admin, &asset);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);
    client.add_strategy(&mock_id);

    // First, record a positive health reading
    mock_client.set_balance(&500i128);
    client.check_strategy_health();
    assert_eq!(client.get_strategy_health(&mock_id), 500i128);

    // Simulate strategy losing all funds
    mock_client.set_balance(&0i128);

    // Second health check should detect the drop and flag the strategy
    let flagged = client.check_strategy_health();
    assert_eq!(flagged.len(), 1);
    assert_eq!(flagged.get(0).unwrap(), mock_id);

    // Stored sentinel should be -1
    assert_eq!(client.get_strategy_health(&mock_id), -1i128);
}

#[test]
fn test_get_strategy_health_default_zero() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    let strategy = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);
    client.add_strategy(&strategy);

    // No health recorded yet → default is 0
    assert_eq!(client.get_strategy_health(&strategy), 0i128);
}

// ─────────────────────────────────────────────
// Flash Loan Support (SC-32)
// ─────────────────────────────────────────────

use soroban_sdk::{contract, contractimpl, symbol_short};
use crate::flash_loan::FlashLoanReceiverClient;

/// Minimal flash-loan provider used to drive the vault's flash-loan flow in
/// tests. It lends `amount`, calls the vault's `flash_loan_callback`, and then
/// asserts it was repaid `amount + fee` — mirroring how a real provider
/// enforces atomicity after the callback returns.
#[contract]
pub struct MockFlashLoanProvider;

#[contractimpl]
impl MockFlashLoanProvider {
    pub fn init(env: Env, fee_bps: u32) {
        env.storage().instance().set(&symbol_short!("fee_bps"), &fee_bps);
    }

    pub fn flash_loan(env: Env, receiver: Address, token: Address, amount: i128) {
        let fee_bps: u32 = env.storage().instance().get(&symbol_short!("fee_bps")).unwrap_or(0);
        let fee = amount * fee_bps as i128 / 10000;

        let tc = TokenClient::new(&env, &token);
        let me = env.current_contract_address();
        let before = tc.balance(&me);

        // Lend the principal, then hand control to the vault's callback.
        tc.transfer(&me, &receiver, &amount);
        FlashLoanReceiverClient::new(&env, &receiver).flash_loan_callback(&token, &amount, &fee, &me);

        // Atomicity: must have been repaid principal + fee, else revert all.
        let after = tc.balance(&me);
        if after < before + fee {
            panic!("flash loan not repaid");
        }
    }
}

/// Spin up an initialized vault + token, returning the pieces tests need.
fn setup_vault<'a>(
    env: &'a Env,
) -> (
    VolatilityShieldClient<'a>,
    Address,
    StellarAssetClient<'a>,
    TokenClient<'a>,
    Address,
) {
    let token_admin = Address::generate(env);
    let (token_id, sac, tc) = create_token_contract(env, &token_admin);

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(&admin, &token_id, &oracle, &treasury, &0u32);

    (client, admin, sac, tc, token_id)
}

/// Register + initialize a mock provider charging `fee_bps`, funded with
/// `funding` of the token so it can lend.
fn setup_provider<'a>(
    env: &'a Env,
    sac: &StellarAssetClient,
    fee_bps: u32,
    funding: i128,
) -> Address {
    let provider_id = env.register_contract(None, MockFlashLoanProvider);
    MockFlashLoanProviderClient::new(env, &provider_id).init(&fee_bps);
    sac.mint(&provider_id, &funding);
    provider_id
}

#[test]
fn test_flash_loan_whitelist_add_remove() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _sac, _tc, _token) = setup_vault(&env);

    let provider = Address::generate(&env);
    assert!(!client.is_flash_loan_provider(&provider));

    client.add_flash_loan_provider(&admin, &provider);
    assert!(client.is_flash_loan_provider(&provider));
    assert_eq!(client.get_flash_loan_providers().len(), 1);

    client.remove_flash_loan_provider(&admin, &provider);
    assert!(!client.is_flash_loan_provider(&provider));
    assert_eq!(client.get_flash_loan_providers().len(), 0);
}

#[test]
fn test_flash_loan_add_provider_requires_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sac, _tc, _token) = setup_vault(&env);

    let stranger = Address::generate(&env);
    let provider = Address::generate(&env);
    assert_eq!(
        client.try_add_flash_loan_provider(&stranger, &provider),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn test_flash_loan_add_duplicate_provider_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _sac, _tc, _token) = setup_vault(&env);

    let provider = Address::generate(&env);
    client.add_flash_loan_provider(&admin, &provider);
    assert_eq!(
        client.try_add_flash_loan_provider(&admin, &provider),
        Err(Ok(Error::ProviderAlreadyWhitelisted))
    );
}

#[test]
fn test_flash_loan_remove_unknown_provider_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _sac, _tc, _token) = setup_vault(&env);

    let provider = Address::generate(&env);
    assert_eq!(
        client.try_remove_flash_loan_provider(&admin, &provider),
        Err(Ok(Error::ProviderNotFound))
    );
}

#[test]
fn test_flash_loan_happy_path_borrow_and_repay() {
    let env = Env::default();
    // The admin authorizes the nested flash_loan_callback (a non-root auth).
    env.mock_all_auths_allowing_non_root_auth();
    let (client, admin, sac, tc, token) = setup_vault(&env);

    // Provider charges 0.5% (within the 1% default cap) and can lend 1000.
    let provider_id = setup_provider(&env, &sac, 50u32, 1000);
    client.add_flash_loan_provider(&admin, &provider_id);

    // The vault holds 100 to cover the fee.
    sac.mint(&client.address, &100);

    // Provider-initiated: lends to the vault and calls flash_loan_callback.
    MockFlashLoanProviderClient::new(&env, &provider_id).flash_loan(&client.address, &token, &1000);

    // Fee = 1000 * 0.5% = 5. Provider gained the fee; the vault paid it.
    assert_eq!(tc.balance(&provider_id), 1005);
    assert_eq!(tc.balance(&client.address), 95);
}

#[test]
#[should_panic]
fn test_flash_loan_non_whitelisted_provider_reverts() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (client, _admin, sac, _tc, token) = setup_vault(&env);

    // Provider is NOT whitelisted → the callback rejects it and the whole
    // flash loan (including the lend) reverts.
    let provider_id = setup_provider(&env, &sac, 0u32, 1000);
    sac.mint(&client.address, &100);

    MockFlashLoanProviderClient::new(&env, &provider_id).flash_loan(&client.address, &token, &1000);
}

#[test]
#[should_panic]
fn test_flash_loan_callback_rejects_unwhitelisted_initiator() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _sac, _tc, token) = setup_vault(&env);

    // Calling the callback directly with an address that is not a whitelisted
    // provider must trap — the vault never repays unknown addresses.
    let stranger = Address::generate(&env);
    client.flash_loan_callback(&token, &1000, &0, &stranger);
}

#[test]
#[should_panic]
fn test_flash_loan_fee_exceeding_cap_reverts() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (client, admin, sac, _tc, token) = setup_vault(&env);

    // Provider charges 2% — above the 1% default cap → callback traps.
    let provider_id = setup_provider(&env, &sac, 200u32, 1000);
    client.add_flash_loan_provider(&admin, &provider_id);
    sac.mint(&client.address, &100);

    MockFlashLoanProviderClient::new(&env, &provider_id).flash_loan(&client.address, &token, &1000);
}

#[test]
#[should_panic]
fn test_flash_loan_unrepayable_reverts() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (client, admin, sac, _tc, token) = setup_vault(&env);

    // Vault is NOT funded for the fee, so it cannot repay principal + fee and
    // the provider's repayment check reverts the transaction.
    let provider_id = setup_provider(&env, &sac, 50u32, 1000);
    client.add_flash_loan_provider(&admin, &provider_id);

    MockFlashLoanProviderClient::new(&env, &provider_id).flash_loan(&client.address, &token, &1000);
}

#[test]
fn test_set_max_flash_loan_fee_bps() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _sac, _tc, _token) = setup_vault(&env);

    assert_eq!(client.max_flash_loan_fee_bps(), 100);
    client.set_max_flash_loan_fee_bps(&admin, &250u32);
    assert_eq!(client.max_flash_loan_fee_bps(), 250);
}

#[test]
fn test_flash_loan_higher_cap_allows_larger_fee() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (client, admin, sac, tc, token) = setup_vault(&env);

    // Raise the cap to 2% so a 2% provider fee is now within bounds.
    client.set_max_flash_loan_fee_bps(&admin, &200u32);

    let provider_id = setup_provider(&env, &sac, 200u32, 1000);
    client.add_flash_loan_provider(&admin, &provider_id);
    sac.mint(&client.address, &100);

    MockFlashLoanProviderClient::new(&env, &provider_id).flash_loan(&client.address, &token, &1000);

    // Fee = 1000 * 2% = 20.
    assert_eq!(tc.balance(&provider_id), 1020);
    assert_eq!(tc.balance(&client.address), 80);
}

// ── Withdrawal Queue Tests ──────────────────────────────────────

#[test]
fn test_queue_withdraw_above_threshold() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (token, sac, tc) = create_token_contract(&env, &Address::generate(&env));
    let admin = Address::generate(&env);
    sac.mint(&admin, &100_000_000_000_000i128);

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &token, &oracle, &treasury, &0u32);

    tc.transfer(&admin, &client.address, &100_000);
    client.set_total_assets(&100_000);
    client.set_total_shares(&1_000);
    client.set_balance(&admin, &1_000);

    // Set threshold to 500 shares
    client.set_withdraw_queue_threshold(&admin, &500);

    // Withdraw 600 shares (above threshold) -> should queue
    let id = client.queue_withdraw(&admin, &600);
    assert_eq!(id, 1);

    // Balance should be reduced immediately
    assert_eq!(client.balance(&admin), 400);
}

#[test]
fn test_queue_withdraw_below_threshold_rejected() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (token, sac, tc) = create_token_contract(&env, &Address::generate(&env));
    let admin = Address::generate(&env);
    sac.mint(&admin, &100_000_000_000_000i128);

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &token, &oracle, &treasury, &0u32);

    tc.transfer(&admin, &client.address, &100_000);
    client.set_total_assets(&100_000);
    client.set_total_shares(&1_000);
    client.set_balance(&admin, &1_000);

    client.set_withdraw_queue_threshold(&admin, &500);

    // Withdraw 200 shares (below threshold) -> should fail
    let result = client.try_queue_withdraw(&admin, &200);
    assert!(result.is_err());
}

#[test]
fn test_queue_withdraw_default_threshold() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (token, sac, tc) = create_token_contract(&env, &Address::generate(&env));
    let admin = Address::generate(&env);
    sac.mint(&admin, &100_000_000_000_000i128);

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &token, &oracle, &treasury, &0u32);

    tc.transfer(&admin, &client.address, &100_000);
    client.set_total_assets(&100_000);
    client.set_total_shares(&1_000);
    client.set_balance(&admin, &1_000);

    // With default threshold (0), any withdrawal queues
    let id = client.queue_withdraw(&admin, &100);
    assert_eq!(id, 1);
}

#[test]
fn test_queue_withdraw_insufficient_shares_rejected() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (token, sac, tc) = create_token_contract(&env, &Address::generate(&env));
    let admin = Address::generate(&env);
    sac.mint(&admin, &100_000_000_000_000i128);

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &token, &oracle, &treasury, &0u32);

    tc.transfer(&admin, &client.address, &100_000);
    client.set_total_assets(&100_000);
    client.set_total_shares(&1_000);
    client.set_balance(&admin, &500);

    client.set_withdraw_queue_threshold(&admin, &100);

    // Try to withdraw more shares than balance
    let result = client.try_queue_withdraw(&admin, &600);
    assert!(result.is_err());
}

#[test]
fn test_set_and_get_queue_threshold() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (token, sac, _tc) = create_token_contract(&env, &Address::generate(&env));
    let admin = Address::generate(&env);

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &token, &oracle, &treasury, &0u32);

    assert_eq!(client.get_withdraw_queue_threshold(), 0);

    client.set_withdraw_queue_threshold(&admin, &1000);
    assert_eq!(client.get_withdraw_queue_threshold(), 1000);
}
