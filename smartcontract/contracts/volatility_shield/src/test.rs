#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{testutils::Address as _, testutils::Ledger, Address, Env, Map};

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

// ── Deposit & Withdrawal Cap Tests ─────────────────────

#[test]
fn test_set_deposit_cap() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    // Set caps
    client.set_deposit_cap(&1000, &5000);

    let (per_user, global) = client.get_deposit_cap();
    assert_eq!(per_user, 1000);
    assert_eq!(global, 5000);
}

#[test]
fn test_set_withdraw_cap() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    client.set_withdraw_cap(&500);
    assert_eq!(client.get_withdraw_cap(), 500);
}

#[test]
#[should_panic(expected = "deposit exceeds per-user cap")]
fn test_deposit_exceeds_per_user_cap() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let token_admin = Address::generate(&env);
    let (token_id, stellar_asset_client, _token_client) = create_token_contract(&env, &token_admin);

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &token_id, &oracle, &treasury, &0u32);
    client.set_deposit_cap(&100, &10000); // per-user cap = 100

    let user = Address::generate(&env);
    stellar_asset_client.mint(&user, &200);

    // First deposit of 60 should succeed
    client.deposit(&user, &60);
    assert_eq!(client.get_user_deposited(&user), 60);

    // Second deposit of 50 should fail (60 + 50 = 110 > 100)
    client.deposit(&user, &50);
}

#[test]
#[should_panic(expected = "deposit exceeds global cap")]
fn test_deposit_exceeds_global_cap() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let token_admin = Address::generate(&env);
    let (token_id, stellar_asset_client, _token_client) = create_token_contract(&env, &token_admin);

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &token_id, &oracle, &treasury, &0u32);
    client.set_deposit_cap(&10000, &200); // global cap = 200

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    stellar_asset_client.mint(&user1, &200);
    stellar_asset_client.mint(&user2, &200);

    // User1 deposits 150
    client.deposit(&user1, &150);

    // User2 tries to deposit 100 (total would be 250 > 200)
    client.deposit(&user2, &100);
}

#[test]
fn test_deposit_at_exact_cap() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let token_admin = Address::generate(&env);
    let (token_id, stellar_asset_client, _token_client) = create_token_contract(&env, &token_admin);

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &token_id, &oracle, &treasury, &0u32);
    client.set_deposit_cap(&100, &500); // per-user cap = 100, global = 500

    let user = Address::generate(&env);
    stellar_asset_client.mint(&user, &200);

    // Deposit exactly at the per-user cap should succeed
    client.deposit(&user, &100);
    assert_eq!(client.get_user_deposited(&user), 100);
    assert_eq!(client.total_assets(), 100);
}

#[test]
#[should_panic(expected = "withdrawal exceeds per-transaction cap")]
fn test_withdraw_exceeds_per_tx_cap() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let token_admin = Address::generate(&env);
    let (token_id, stellar_asset_client, _token_client) = create_token_contract(&env, &token_admin);

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &token_id, &oracle, &treasury, &0u32);

    // Setup vault with 1:1 ratio
    client.set_total_shares(&1000);
    client.set_total_assets(&1000);

    let user = Address::generate(&env);
    client.set_balance(&user, &500);
    stellar_asset_client.mint(&contract_id, &1000);

    // Set withdraw cap to 100 per tx
    client.set_withdraw_cap(&100);

    // Withdraw 200 shares => 200 assets, exceeds 100 cap
    client.withdraw(&user, &200);
}

#[test]
fn test_withdraw_within_cap() {
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

    // Setup vault with 1:1 ratio
    client.set_total_shares(&1000);
    client.set_total_assets(&1000);

    let user = Address::generate(&env);
    client.set_balance(&user, &500);
    stellar_asset_client.mint(&contract_id, &1000);

    // Set withdraw cap to 100 per tx
    client.set_withdraw_cap(&100);

    // Withdraw 50 shares => 50 assets, within cap
    client.withdraw(&user, &50);
    assert_eq!(client.balance(&user), 450);
    assert_eq!(token_client.balance(&user), 50);
}

#[test]
fn test_caps_not_set_allows_unlimited() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let token_admin = Address::generate(&env);
    let (token_id, stellar_asset_client, _token_client) = create_token_contract(&env, &token_admin);

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &token_id, &oracle, &treasury, &0u32);

    // No caps set — defaults should be (0, 0) and 0
    let (per_user, global) = client.get_deposit_cap();
    assert_eq!(per_user, 0);
    assert_eq!(global, 0);
    assert_eq!(client.get_withdraw_cap(), 0);

    let user = Address::generate(&env);
    stellar_asset_client.mint(&user, &1_000_000);

    // Large deposit should succeed with no caps
    client.deposit(&user, &1_000_000);
    assert_eq!(client.total_assets(), 1_000_000);
}

#[test]
fn test_multiple_deposits_track_cumulative() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let token_admin = Address::generate(&env);
    let (token_id, stellar_asset_client, _token_client) = create_token_contract(&env, &token_admin);

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &token_id, &oracle, &treasury, &0u32);
    client.set_deposit_cap(&500, &10000);

    let user = Address::generate(&env);
    stellar_asset_client.mint(&user, &1000);

    // Deposit in 3 batches
    client.deposit(&user, &100);
    assert_eq!(client.get_user_deposited(&user), 100);

    client.deposit(&user, &200);
    assert_eq!(client.get_user_deposited(&user), 300);

    client.deposit(&user, &150);
    assert_eq!(client.get_user_deposited(&user), 450);

    // Total deposited = 450, next 60 would exceed 500 cap
    // Verify the balance is tracked correctly
    assert_eq!(client.total_assets(), 450);
}

// ── Timelock Tests ───────────────────────────

#[test]
fn test_set_timelock_duration() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    client.set_timelock_duration(&86400u64);
    assert_eq!(client.get_timelock_duration(), 86400u64);
}

#[test]
fn test_propose_action_stores_timestamp() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1000);

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);
    client.set_timelock_duration(&100u64);

    let timestamp = client.propose_action();
    assert_eq!(timestamp, 1000);
    assert_eq!(client.get_timelock_proposal_timestamp(), timestamp);
}

#[test]
#[should_panic(expected = "timelock duration not set")]
fn test_propose_action_without_duration_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    client.propose_action();
}

#[test]
#[should_panic(expected = "timelock not elapsed")]
fn test_execute_action_before_timelock_expires_panics() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1000);

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);
    client.set_timelock_duration(&86400u64);

    client.propose_action();

    client.execute_action();
}

#[test]
fn test_execute_action_after_timelock_expires() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1000);

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);
    client.set_timelock_duration(&100u64);

    let timestamp = client.propose_action();
    assert_eq!(timestamp, 1000);

    env.ledger().set_timestamp(timestamp + 101);

    let execution_timestamp = client.execute_action();
    assert!(execution_timestamp > timestamp);
}

#[test]
#[should_panic(expected = "timelock not set")]
fn test_execute_action_without_proposal_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);
    client.set_timelock_duration(&100u64);

    client.execute_action();
}

#[test]
fn test_timelock_default_duration_is_zero() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    assert_eq!(client.get_timelock_duration(), 0u64);
}

#[test]
fn test_get_timelock_proposal_timestamp_default() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    assert_eq!(client.get_timelock_proposal_timestamp(), 0u64);
}
