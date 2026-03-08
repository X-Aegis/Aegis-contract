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
fn test_init_comprehensive() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    let fee_percentage = 250u32; // 2.5%

    client.init(&admin, &asset, &oracle, &treasury, &fee_percentage);

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_asset(), asset);
    assert_eq!(client.get_oracle(), oracle);
    assert_eq!(client.treasury(), treasury);
    assert_eq!(client.fee_percentage(), fee_percentage);
    assert_eq!(client.get_strategies().len(), 0);
}

#[test]
#[should_panic(expected = "Already initialized")]
fn test_init_already_initialized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);
    // Should panic
    client.init(&admin, &asset, &oracle, &treasury, &0u32);
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

#[test]
fn test_deposit_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let (token_id, stellar_asset_client, token_client) = create_token_contract(&env, &token_admin);

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &token_id, &oracle, &treasury, &0u32);

    let user = Address::generate(&env);
    let deposit_amount = 1000i128;

    // Mint tokens to user
    stellar_asset_client.mint(&user, &deposit_amount);
    assert_eq!(token_client.balance(&user), deposit_amount);

    // Initial state check
    assert_eq!(client.total_shares(), 0);
    assert_eq!(client.total_assets(), 0);

    // Perform deposit
    client.deposit(&user, &deposit_amount);

    // Verify balances after deposit
    assert_eq!(token_client.balance(&user), 0);
    assert_eq!(token_client.balance(&contract_id), deposit_amount);
    assert_eq!(client.balance(&user), deposit_amount); // 1:1 since total_shares was 0
    assert_eq!(client.total_shares(), deposit_amount);
    assert_eq!(client.total_assets(), deposit_amount);
}

#[test]
#[should_panic(expected = "deposit amount must be positive")]
fn test_deposit_amount_zero() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    client.deposit(&user, &0);
}

#[test]
fn test_fuzz_math_symmetry() {
    proptest::proptest!(
        proptest::prelude::ProptestConfig::with_cases(100),
        |(
            amount in 1..1_000_000_000_000i128,
            total_assets in 1..1_000_000_000_000i128,
            total_shares in 1..1_000_000_000_000i128
        )| {
            let env = Env::default();
            let contract_id = env.register_contract(None, VolatilityShield);
            let client = VolatilityShieldClient::new(&env, &contract_id);
            let admin = Address::generate(&env);
            let asset = Address::generate(&env);
            let oracle = Address::generate(&env);
            let treasury = Address::generate(&env);
            client.init(&admin, &asset, &oracle, &treasury, &0u32);

            client.set_total_assets(&total_assets);
            client.set_total_shares(&total_shares);

            let shares = client.convert_to_shares(&amount);
            let assets_back = client.convert_to_assets(&shares);

            // Property: Assets back should be <= original amount (rounding favors the vault)
            if assets_back > amount {
                panic!("Rounding error: assets_back {} > amount {}", assets_back, amount);
            }
        }
    );
}

#[test]
fn test_fuzz_conversion_no_panic() {
    proptest::proptest!(
        proptest::prelude::ProptestConfig::with_cases(100),
        |(
            amount in 0..1_000_000_000_000_000_000_000_000i128, // 10^24
            total_assets in 1..1_000_000_000_000_000_000_000_000i128,
            total_shares in 1..1_000_000_000_000_000_000_000_000i128
        )| {
            let env = Env::default();
            let contract_id = env.register_contract(None, VolatilityShield);
            let client = VolatilityShieldClient::new(&env, &contract_id);
            let admin = Address::generate(&env);
            let asset = Address::generate(&env);
            let oracle = Address::generate(&env);
            let treasury = Address::generate(&env);
            client.init(&admin, &asset, &oracle, &treasury, &0u32);

            client.set_total_assets(&total_assets);
            client.set_total_shares(&total_shares);

            client.convert_to_shares(&amount);
            client.convert_to_assets(&amount);
        }
    );
}
