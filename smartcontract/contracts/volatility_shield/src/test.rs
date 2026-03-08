#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{testutils::Address as _, testutils::Ledger, Address, Env, IntoVal, Map};

// Import MockStrategy for integration testing
#[cfg(test)]
use mock_strategy::{MockStrategy, MockStrategyClient};

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
fn test_withdraw_flow_complete() {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let (token_id, stellar_asset_client, token_client) = create_token_contract(&env, &token_admin);

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    let fee_bps = 1000u32; // 10% fee for easier math

    client.init(&admin, &token_id, &oracle, &treasury, &fee_bps);

    let user = Address::generate(&env);
    let deposit_amount = 1000i128;

    // 1. Setup: Deposit 1000 tokens
    stellar_asset_client.mint(&user, &deposit_amount);
    client.deposit(&user, &deposit_amount);

    assert_eq!(client.balance(&user), 1000);
    assert_eq!(client.total_shares(), 1000);
    assert_eq!(client.total_assets(), 1000);

    // 2. Perform Withdrawal of 500 shares
    // At 1:1 ratio, 500 shares = 500 assets
    // 10% fee of 500 = 50 assets
    // Net to user = 450 assets
    client.withdraw(&user, &500);

    // 3. Verify balances after withdrawal
    assert_eq!(client.balance(&user), 500);
    assert_eq!(client.total_shares(), 500);
    assert_eq!(client.total_assets(), 500); // total_assets is reduced by full assets_to_withdraw (500)
    
    assert_eq!(token_client.balance(&user), 450);
    assert_eq!(token_client.balance(&treasury), 50);
    assert_eq!(token_client.balance(&contract_id), 500);
}

#[test]
#[should_panic(expected = "insufficient shares for withdrawal")]
fn test_withdraw_insufficient_shares() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    
    client.withdraw(&user, &1);
}

#[test]
#[should_panic(expected = "shares to withdraw must be positive")]
fn test_withdraw_amount_zero() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    client.withdraw(&user, &0);
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
    client.rebalance(&allocations, &0u32);
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

// ── Multisig Flow Tests ───────────────────────────────

#[test]
fn test_multisig_flow_set_paused() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    let g1 = Address::generate(&env);
    let g2 = Address::generate(&env);
    let guardians = soroban_sdk::vec![&env, g1.clone(), g2.clone()];
    
    // Initialize multisig with 2 guardians and threshold 2
    client.init_multisig(&guardians, &2u32);

    // 1. Propose SetPaused(true)
    let action_data = soroban_sdk::vec![&env, true.into_val(&env)];
    let proposal_id = client.propose_multisig_action(&g1, &ActionType::SetPaused, &soroban_sdk::String::from_str(&env, "Pause for maintenance"), &action_data);

    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.action_type, ActionType::SetPaused);
    assert_eq!(proposal.executed, false);

    // 2. First approval (threshold 2 not met)
    client.approve_multisig_action(&g1, &proposal_id);
    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.executed, false);

    // 3. Second approval (executes)
    client.approve_multisig_action(&g2, &proposal_id);
    let proposal = client.get_proposal(&proposal_id);
    assert_eq!(proposal.executed, true);

    // Verify contract is actually paused
    let is_paused: bool = env.as_contract(&contract_id, || {
        env.storage().instance().get(&DataKey::Paused).unwrap_or(false)
    });
    assert!(is_paused);
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
#[should_panic(expected = "set_paused must go through multisig proposal")]
fn test_set_paused_fails_when_multisig_enabled() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VolatilityShield);
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    let g1 = Address::generate(&env);
    let guardians = soroban_sdk::vec![&env, g1.clone()];
    client.init_multisig(&guardians, &1u32);

    // Direct call should fail
    client.set_paused(&true);
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
    let (token_id, stellar_asset_client, token_client) = create_token_contract(&env, &token_admin);

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
    let (token_id, stellar_asset_client, token_client) = create_token_contract(&env, &token_admin);

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
    let (token_id, stellar_asset_client, token_client) = create_token_contract(&env, &token_admin);

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
    let (token_id, stellar_asset_client, token_client) = create_token_contract(&env, &token_admin);

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
    let (token_id, stellar_asset_client, token_client) = create_token_contract(&env, &token_admin);

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

// ── Slippage Protection Tests ─────────────────

#[test]
fn test_rebalance_with_zero_slippage_tolerance() {
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
    client.rebalance(&allocations, &0u32);
}

// ── MockStrategy Integration Tests ───────────────

#[test]
fn test_mock_strategy_integration() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    // Register contracts
    let volatility_shield_id = env.register_contract(None, VolatilityShield);
    let volatility_shield_client = VolatilityShieldClient::new(&env, &volatility_shield_id);

    let mock_strategy_id = env.register_contract(None, MockStrategy);
    let mock_strategy_client = MockStrategyClient::new(&env, &mock_strategy_id);

    // Create token
    let token_admin = Address::generate(&env);
    let (token_id, stellar_asset_client, token_client) = create_token_contract(&env, &token_admin);

    // Initialize contracts
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    volatility_shield_client.init(&admin, &token_id, &oracle, &treasury, &0u32);
    mock_strategy_client.init(&admin, &token_id);

    // Add strategy to VolatilityShield
    volatility_shield_client.add_strategy(&mock_strategy_id);

    // Mint tokens to VolatilityShield for testing
    stellar_asset_client.mint(&volatility_shield_id, &1000);

    // Test rebalancing - move funds to MockStrategy
    let mut allocations: Map<Address, i128> = Map::new(&env);
    allocations.set(mock_strategy_id.clone(), 500);

    volatility_shield_client.rebalance(&allocations, &0u32);

    // Verify MockStrategy balance
    assert_eq!(mock_strategy_client.balance(), 500);

    // Test rebalancing - move funds back from MockStrategy
    let mut allocations: Map<Address, i128> = Map::new(&env);
    allocations.set(mock_strategy_id, 200);

    volatility_shield_client.rebalance(&allocations, &0u32);

    // Verify updated balance
    assert_eq!(mock_strategy_client.balance(), 200);
}

#[test]
fn test_mock_strategy_deposit_withdraw_flow() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    // Register contracts
    let volatility_shield_id = env.register_contract(None, VolatilityShield);
    let volatility_shield_client = VolatilityShieldClient::new(&env, &volatility_shield_id);

    let mock_strategy_id = env.register_contract(None, MockStrategy);
    let mock_strategy_client = MockStrategyClient::new(&env, &mock_strategy_id);

    // Create token
    let token_admin = Address::generate(&env);
    let (token_id, stellar_asset_client, token_client) = create_token_contract(&env, &token_admin);

    // Initialize contracts
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    volatility_shield_client.init(&admin, &token_id, &oracle, &treasury, &0u32);
    mock_strategy_client.init(&admin, &token_id);

    // Add strategy to VolatilityShield
    volatility_shield_client.add_strategy(&mock_strategy_id);

    // Mint tokens to VolatilityShield for testing
    stellar_asset_client.mint(&volatility_shield_id, &1000);

    // Test: Move funds to MockStrategy
    let mut allocations: Map<Address, i128> = Map::new(&env);
    allocations.set(mock_strategy_id.clone(), 300);

    volatility_shield_client.rebalance(&allocations, &0u32);

    // Verify funds moved to MockStrategy
    assert_eq!(mock_strategy_client.balance(), 300);

    // Test: Move all funds back from MockStrategy
    let mut allocations: Map<Address, i128> = Map::new(&env);
    allocations.set(mock_strategy_id, 0);

    volatility_shield_client.rebalance(&allocations, &0u32);

    // Verify all funds withdrawn
    assert_eq!(mock_strategy_client.balance(), 0);
}

#[test]
fn test_multiple_mock_strategies() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    // Register contracts
    let volatility_shield_id = env.register_contract(None, VolatilityShield);
    let volatility_shield_client = VolatilityShieldClient::new(&env, &volatility_shield_id);

    let mock_strategy1_id = env.register_contract(None, MockStrategy);
    let mock_strategy1_client = MockStrategyClient::new(&env, &mock_strategy1_id);

    let mock_strategy2_id = env.register_contract(None, MockStrategy);
    let mock_strategy2_client = MockStrategyClient::new(&env, &mock_strategy2_id);

    // Create token
    let token_admin = Address::generate(&env);
    let (token_id, stellar_asset_client, token_client) = create_token_contract(&env, &token_admin);

    // Initialize contracts
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    volatility_shield_client.init(&admin, &token_id, &oracle, &treasury, &0u32);
    mock_strategy1_client.init(&admin, &token_id);
    mock_strategy2_client.init(&admin, &token_id);

    // Add strategies to VolatilityShield
    volatility_shield_client.add_strategy(&mock_strategy1_id);
    volatility_shield_client.add_strategy(&mock_strategy2_id);

    // Mint tokens to VolatilityShield for testing
    stellar_asset_client.mint(&volatility_shield_id, &1000);

    // Test: Distribute funds across strategies
    let mut allocations: Map<Address, i128> = Map::new(&env);
    allocations.set(mock_strategy1_id.clone(), 400);
    allocations.set(mock_strategy2_id.clone(), 600);

    volatility_shield_client.rebalance(&allocations, &0u32);

    // Verify balances
    assert_eq!(mock_strategy1_client.balance(), 400);
    assert_eq!(mock_strategy2_client.balance(), 600);

    // Test: Rebalance to different allocation
    let mut allocations: Map<Address, i128> = Map::new(&env);
    allocations.set(mock_strategy1_id, 300);
    allocations.set(mock_strategy2_id, 700);

    volatility_shield_client.rebalance(&allocations, &0u32);

    // Verify updated balances
    assert_eq!(mock_strategy1_client.balance(), 300);
    assert_eq!(mock_strategy2_client.balance(), 700);
}
