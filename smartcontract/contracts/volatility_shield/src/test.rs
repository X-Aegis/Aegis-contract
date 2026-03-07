#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{testutils::Address as _, Address, Env, Map};

fn create_token_contract<'a>(
    env: &Env,
    admin: &Address,
) -> (Address, StellarAssetClient<'a>, TokenClient<'a>) {
    let contract_id = env.register_stellar_asset_contract_v2(admin.clone());
    let stellar_asset_client = StellarAssetClient::new(env, &contract_id.address());
    let token_client = TokenClient::new(env, &contract_id.address());
    (contract_id.address(), stellar_asset_client, token_client)
}

#[test]
fn test_init_stores_roles() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &500u32);

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_oracle(), oracle);
    assert_eq!(client.get_asset(), asset);
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
fn test_withdraw_with_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let (token_id, stellar_asset_client, token_client) = create_token_contract(&env, &token_admin);

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    // 5% fee (500 basis points)
    client.init(&admin, &token_id, &oracle, &treasury, &500u32);

    // Setup vault state
    client.set_total_shares(&1000);
    client.set_total_assets(&1000); // 1:1 ratio for simplicity

    let user = Address::generate(&env);
    client.set_balance(&user, &100);

    // Mint assets to contract
    stellar_asset_client.mint(&contract_id, &1000);

    // Withdraw 100 shares -> 100 assets
    // 5% of 100 = 5 assets fee
    // User gets 95 assets
    client.withdraw(&user, &100);

    assert_eq!(client.balance(&user), 0);
    assert_eq!(token_client.balance(&user), 95);
    assert_eq!(token_client.balance(&treasury), 5);
    assert_eq!(client.total_assets(), 900);
}

#[test]
fn test_rebalance_admin_auth_accepted() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    let allocations: Map<Address, i128> = Map::new(&env);
    client.rebalance(&allocations);
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn test_pause_circuit_breaker() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    // Pause the contract
    client.set_paused(&true);

    let user = Address::generate(&env);

    // This should panic because the contract is paused
    client.deposit(&user, &100);
}


// ── Rebalance Delta Calculation Tests ─────────────────

#[test]
fn test_calc_rebalance_delta_positive() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    // Current is 100, Target is 150. Delta should be +50
    let delta = client.calc_rebalance_delta(&100, &150);
    assert_eq!(delta, 50);
}

#[test]
fn test_calc_rebalance_delta_negative() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    // Current is 200, Target is 50. Delta should be -150
    let delta = client.calc_rebalance_delta(&200, &50);
    assert_eq!(delta, -150);
}

#[test]
fn test_calc_rebalance_delta_identical() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    // Current matches Target. Delta should be 0.
    let delta = client.calc_rebalance_delta(&100, &100);
    assert_eq!(delta, 0);
}

#[test]
fn test_calc_rebalance_delta_zero_current() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    // Current is 0, Target is 100. Delta should be +100.
    let delta = client.calc_rebalance_delta(&0, &100);
    assert_eq!(delta, 100);
}

#[test]
fn test_calc_rebalance_delta_zero_target() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    // Current is 100, Target is 0. Delta should be -100.
    let delta = client.calc_rebalance_delta(&100, &0);
    assert_eq!(delta, -100);
}

#[test]
#[should_panic(expected = "Balances cannot be negative")]
fn test_calc_rebalance_delta_negative_inputs_panic() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    // Should panic on negative balances
    client.calc_rebalance_delta(&-50, &100);
}