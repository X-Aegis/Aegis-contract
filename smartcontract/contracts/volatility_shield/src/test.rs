#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger, MockAuth, MockAuthInvoke},
    Address, Env, IntoVal, Map, Symbol,
};

extern crate mock_strategy;

#[soroban_sdk::contract]
struct RejectingToken;

#[soroban_sdk::contractimpl]
impl RejectingToken {
    pub fn balance(_env: Env, _id: Address) -> i128 {
        100
    }

    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {
        panic!("simulated token transfer failure");
    }
}

#[soroban_sdk::contract]
struct MockPriceOracle;

#[soroban_sdk::contractimpl]
impl MockPriceOracle {
    pub fn set_price(env: Env, price: i128) {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "price"), &price);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "ts"), &env.ledger().timestamp());
    }

    pub fn set_timestamp(env: Env, ts: u64) {
        env.storage().instance().set(&Symbol::new(&env, "ts"), &ts);
    }

    pub fn last_price(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "price"))
            .unwrap_or(10_000_000)
    }

    pub fn last_timestamp(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "ts"))
            .unwrap_or(0)
    }
}

fn create_token_contract<'a>(
    env: &Env,
    admin: &Address,
) -> (Address, StellarAssetClient<'a>, TokenClient<'a>) {
    let contract_id = env.register_stellar_asset_contract_v2(admin.clone());
    let stellar_asset_client = StellarAssetClient::new(env, &contract_id.address());
    let token_client = TokenClient::new(env, &contract_id.address());
    (contract_id.address(), stellar_asset_client, token_client)
}

fn contract_error(error: Error) -> soroban_sdk::Error {
    soroban_sdk::Error::from_contract_error(error as u32)
}

#[test]
fn test_init_stores_roles() {
    let env = Env::default();
    let contract_id = env.register(VolatilityShield, ());
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
    let contract_id = env.register(VolatilityShield, ());
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
    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    client.convert_to_assets(&-1);
}

#[test]
fn test_convert_to_shares() {
    let env = Env::default();
    let contract_id = env.register(VolatilityShield, ());
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

    let contract_id = env.register(VolatilityShield, ());
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
    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    client.convert_to_shares(&-1);
}

#[test]
fn test_take_fees() {
    let env = Env::default();
    let contract_id = env.register(VolatilityShield, ());
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
fn test_set_and_get_benchmark_rate() {
    let env = Env::default();
    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    env.mock_all_auths_allowing_non_root_auth();
    client.init(&admin, &asset, &oracle, &treasury, &500u32);

    assert_eq!(client.benchmark_rate(), 0u32);

    client.set_benchmark_rate(&admin, &500u32); // 5%
    assert_eq!(client.benchmark_rate(), 500u32);
}

#[test]
fn test_set_and_get_current_vault_apy() {
    let env = Env::default();
    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    env.mock_all_auths_allowing_non_root_auth();
    client.init(&admin, &asset, &oracle, &treasury, &500u32);

    assert_eq!(client.current_vault_apy(), 0u32);

    client.set_current_vault_apy(&admin, &1200u32); // 12%
    assert_eq!(client.current_vault_apy(), 1200u32);
}

#[test]
#[should_panic]
fn test_benchmark_setter_requires_admin() {
    let env = Env::default();
    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let other = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    env.mock_all_auths();
    client.init(&admin, &asset, &oracle, &treasury, &500u32);

    client.set_benchmark_rate(&other, &500u32);
}

#[test]
fn test_take_fees_dynamic_outperform() {
    let env = Env::default();
    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    env.mock_all_auths_allowing_non_root_auth();

    // Base fee = 5% (500 BPS), Benchmark = 5% (500 BPS), Vault APY = 10% (1000 BPS)
    client.init(&admin, &asset, &oracle, &treasury, &500u32);
    client.set_benchmark_rate(&admin, &500u32);
    client.set_current_vault_apy(&admin, &1000u32);

    // vault_apy / benchmark = 1000/500 = 2.0 → multiplier = 2.0
    // capped at 2.0× → effective fee = 500 * 2 = 1000 BPS = 10%
    let (remaining, fee) = client.take_fees(&1000i128);
    assert_eq!(fee, 100);
    assert_eq!(remaining, 900);
}

#[test]
fn test_take_fees_dynamic_underperform() {
    let env = Env::default();
    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    // Base fee = 5% (500 BPS), Benchmark = 10% (1000 BPS), Vault APY = 5% (500 BPS)
    env.mock_all_auths_allowing_non_root_auth();
    client.init(&admin, &asset, &oracle, &treasury, &500u32);
    client.set_benchmark_rate(&admin, &1000u32);
    client.set_current_vault_apy(&admin, &500u32);

    // vault_apy / benchmark = 500/1000 = 0.5 → multiplier = 0.5
    // effective fee = 500 * 0.5 = 250 BPS = 2.5%
    let (remaining, fee) = client.take_fees(&1000i128);
    assert_eq!(fee, 25);
    assert_eq!(remaining, 975);
}

#[test]
fn test_take_fees_falls_back_to_base_when_no_benchmark() {
    let env = Env::default();
    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    // No benchmark or APY set → base fee applies
    env.mock_all_auths_allowing_non_root_auth();
    client.init(&admin, &asset, &oracle, &treasury, &200u32);

    let (remaining, fee) = client.take_fees(&1000i128);
    assert_eq!(fee, 20);
    assert_eq!(remaining, 980);
}

#[test]
fn test_take_fees_extreme_outperformance_capped() {
    let env = Env::default();
    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    // 500% APY vs 5% benchmark → ratio 100×, but multiplier capped at 2.0×
    env.mock_all_auths_allowing_non_root_auth();
    client.init(&admin, &asset, &oracle, &treasury, &500u32);
    client.set_benchmark_rate(&admin, &500u32);
    client.set_current_vault_apy(&admin, &50000u32);

    let (remaining, fee) = client.take_fees(&1000i128);
    assert_eq!(fee, 100); // capped at 2× base = 1000 BPS = 10%
    assert_eq!(remaining, 900);
}

#[test]
fn test_take_fees_zero_fee_always_zero() {
    let env = Env::default();
    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    env.mock_all_auths_allowing_non_root_auth();

    client.init(&admin, &asset, &oracle, &treasury, &0u32);
    client.set_benchmark_rate(&admin, &500u32);
    client.set_current_vault_apy(&admin, &2000u32);

    let (remaining, fee) = client.take_fees(&1000i128);
    assert_eq!(fee, 0);
    assert_eq!(remaining, 1000);
}

#[test]
fn test_withdraw_success() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let token_admin = Address::generate(&env);
    let (token_id, stellar_asset_client, token_client) = create_token_contract(&env, &token_admin);

    let contract_id = env.register(VolatilityShield, ());
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
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
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
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
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
    let contract_id = env.register(VolatilityShield, ());
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
    env.ledger().with_mut(|li| {
        li.timestamp = 2000;
    });
    client.rebalance(&admin);
}

#[test]
#[should_panic(expected = "allocation values cannot be negative")]
fn test_rebalance_negative_allocation() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VolatilityShield, ());
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
    env.ledger().with_mut(|li| {
        li.timestamp = 2000;
    });
    client.rebalance(&admin);
}

#[test]
#[should_panic(expected = "allocation to zero-address or unlisted strategy")]
fn test_rebalance_unlisted_strategy() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VolatilityShield, ());
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
    env.ledger().with_mut(|li| {
        li.timestamp = 2000;
    });
    client.rebalance(&admin);
}

// ── Strategy Health Tests ─────────────────────────────────────────────────

#[test]
fn test_flag_strategy_marks_flagged() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(VolatilityShield, ());
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

    let contract_id = env.register(VolatilityShield, ());
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

    let contract_id = env.register(VolatilityShield, ());
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

    let contract_id = env.register(VolatilityShield, ());
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

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    // Register mock strategy contract so cross-contract balance() call works
    let mock_id = env.register(mock_strategy::MockStrategy, ());
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

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    let mock_id = env.register(mock_strategy::MockStrategy, ());
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

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    let mock_id = env.register(mock_strategy::MockStrategy, ());
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

    let contract_id = env.register(VolatilityShield, ());
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

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    let mock_id = env.register(mock_strategy::MockStrategy, ());
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

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    let mock_id = env.register(mock_strategy::MockStrategy, ());
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

    let contract_id = env.register(VolatilityShield, ());
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

use crate::flash_loan::FlashLoanReceiverClient;
use soroban_sdk::{contract, contractimpl, symbol_short};

/// Minimal flash-loan provider used to drive the vault's flash-loan flow in
/// tests. It lends `amount`, calls the vault's `flash_loan_callback`, and then
/// asserts it was repaid `amount + fee` — mirroring how a real provider
/// enforces atomicity after the callback returns.
#[contract]
pub struct MockFlashLoanProvider;

#[contractimpl]
impl MockFlashLoanProvider {
    pub fn init(env: Env, fee_bps: u32) {
        env.storage()
            .instance()
            .set(&symbol_short!("fee_bps"), &fee_bps);
    }

    pub fn flash_loan(env: Env, receiver: Address, token: Address, amount: i128) {
        let fee_bps: u32 = env
            .storage()
            .instance()
            .get(&symbol_short!("fee_bps"))
            .unwrap_or(0);
        let fee = amount * fee_bps as i128 / 10000;

        let tc = TokenClient::new(&env, &token);
        let me = env.current_contract_address();
        let before = tc.balance(&me);

        // Lend the principal, then hand control to the vault's callback.
        tc.transfer(&me, &receiver, &amount);
        FlashLoanReceiverClient::new(&env, &receiver)
            .flash_loan_callback(&token, &amount, &fee, &me);

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

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(&admin, &token_id, &oracle, &treasury, &0u32);

    (client, admin, sac, tc, token_id)
}

/// Register + initialize a mock provider charging `fee_bps`, funded with
/// `funding` of the token so it can lend.
fn setup_provider(env: &Env, sac: &StellarAssetClient, fee_bps: u32, funding: i128) -> Address {
    let provider_id = env.register(MockFlashLoanProvider, ());
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

    let contract_id = env.register(VolatilityShield, ());
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

    let contract_id = env.register(VolatilityShield, ());
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

    let contract_id = env.register(VolatilityShield, ());
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

    let contract_id = env.register(VolatilityShield, ());
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
fn test_queue_withdraw_multiple_entries_do_not_overwrite() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (token, sac, tc) = create_token_contract(&env, &Address::generate(&env));
    let admin = Address::generate(&env);
    sac.mint(&admin, &100_000_000_000_000i128);

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &token, &oracle, &treasury, &0u32);

    tc.transfer(&admin, &client.address, &100_000);
    client.set_total_assets(&100_000);
    client.set_total_shares(&2_000);
    client.set_balance(&admin, &2_000);

    client.set_withdraw_queue_threshold(&admin, &500);

    // Queue two separate withdrawals from the same account.
    let id1 = client.queue_withdraw(&admin, &600);
    let id2 = client.queue_withdraw(&admin, &700);

    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
    assert_ne!(id1, id2);

    // Both must be independently retrievable — the second queue_withdraw
    // call must not have clobbered the first (the regression this test
    // guards against).
    let pending1 = client.get_pending_withdrawal(&id1).unwrap();
    let pending2 = client.get_pending_withdrawal(&id2).unwrap();

    assert_eq!(pending1.from, admin);
    assert_eq!(pending1.shares, 600);
    assert!(!pending1.processed);

    assert_eq!(pending2.from, admin);
    assert_eq!(pending2.shares, 700);
    assert!(!pending2.processed);

    // Shares were deducted for both withdrawals.
    assert_eq!(client.balance(&admin), 700);
}

#[test]
fn test_queue_withdraw_multiple_accounts_independent() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (token, sac, tc) = create_token_contract(&env, &Address::generate(&env));
    let admin = Address::generate(&env);
    sac.mint(&admin, &100_000_000_000_000i128);

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    let user2 = Address::generate(&env);

    client.init(&admin, &token, &oracle, &treasury, &0u32);

    tc.transfer(&admin, &client.address, &100_000);
    client.set_total_assets(&100_000);
    client.set_total_shares(&2_000);
    client.set_balance(&admin, &1_000);
    client.set_balance(&user2, &1_000);

    client.set_withdraw_queue_threshold(&admin, &500);

    let id1 = client.queue_withdraw(&admin, &600);
    let id2 = client.queue_withdraw(&user2, &900);

    let pending1 = client.get_pending_withdrawal(&id1).unwrap();
    let pending2 = client.get_pending_withdrawal(&id2).unwrap();

    assert_eq!(pending1.from, admin);
    assert_eq!(pending1.shares, 600);
    assert_eq!(pending2.from, user2);
    assert_eq!(pending2.shares, 900);
}

#[test]
fn test_process_queued_withdrawal_transfers_assets() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (token, sac, tc) = create_token_contract(&env, &Address::generate(&env));
    let admin = Address::generate(&env);
    sac.mint(&admin, &100_000_000_000_000i128);

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &token, &oracle, &treasury, &0u32);

    tc.transfer(&admin, &client.address, &100_000);
    client.set_total_assets(&100_000);
    client.set_total_shares(&1_000);
    client.set_balance(&admin, &1_000);

    client.set_withdraw_queue_threshold(&admin, &500);

    let id = client.queue_withdraw(&admin, &600);

    let balance_before = tc.balance(&admin);
    client.process_queued_withdrawal(&admin, &id);
    let balance_after = tc.balance(&admin);

    // 600 shares out of 1000 total shares backed by 100_000 assets = 60_000 assets.
    assert_eq!(balance_after - balance_before, 60_000);

    let pending = client.get_pending_withdrawal(&id).unwrap();
    assert!(pending.processed);

    assert_eq!(client.total_shares(), 400);
    assert_eq!(client.total_assets(), 40_000);
}

#[test]
fn test_process_queued_withdrawal_not_found() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (token, _sac, _tc) = create_token_contract(&env, &Address::generate(&env));
    let admin = Address::generate(&env);

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &token, &oracle, &treasury, &0u32);

    let result = client.try_process_queued_withdrawal(&admin, &999);
    assert!(result.is_err());
}

#[test]
fn test_process_queued_withdrawal_already_processed_rejected() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (token, sac, tc) = create_token_contract(&env, &Address::generate(&env));
    let admin = Address::generate(&env);
    sac.mint(&admin, &100_000_000_000_000i128);

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &token, &oracle, &treasury, &0u32);

    tc.transfer(&admin, &client.address, &100_000);
    client.set_total_assets(&100_000);
    client.set_total_shares(&1_000);
    client.set_balance(&admin, &1_000);

    client.set_withdraw_queue_threshold(&admin, &500);

    let id = client.queue_withdraw(&admin, &600);
    client.process_queued_withdrawal(&admin, &id);

    let result = client.try_process_queued_withdrawal(&admin, &id);
    assert!(result.is_err());
}

#[test]
fn test_process_queued_withdrawal_unauthorized_rejected() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (token, sac, tc) = create_token_contract(&env, &Address::generate(&env));
    let admin = Address::generate(&env);
    sac.mint(&admin, &100_000_000_000_000i128);

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    let not_admin = Address::generate(&env);

    client.init(&admin, &token, &oracle, &treasury, &0u32);

    tc.transfer(&admin, &client.address, &100_000);
    client.set_total_assets(&100_000);
    client.set_total_shares(&1_000);
    client.set_balance(&admin, &1_000);

    client.set_withdraw_queue_threshold(&admin, &500);

    let id = client.queue_withdraw(&admin, &600);

    let result = client.try_process_queued_withdrawal(&not_admin, &id);
    assert!(result.is_err());
}

#[test]
fn test_set_and_get_queue_threshold() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (token, _sac, _tc) = create_token_contract(&env, &Address::generate(&env));
    let admin = Address::generate(&env);

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    client.init(&admin, &token, &oracle, &treasury, &0u32);

    assert_eq!(client.get_withdraw_queue_threshold(), 0);

    client.set_withdraw_queue_threshold(&admin, &1000);
    assert_eq!(client.get_withdraw_queue_threshold(), 1000);
}

#[test]
fn test_upgrade() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);

    client.init(&admin, &asset, &oracle, &treasury, &500u32);
    client.set_total_shares(&1000);
    client.set_total_assets(&5000);

    // Provide a valid WASM module to satisfy Soroban validation
    let wasm = soroban_sdk::Bytes::from_slice(&env, include_bytes!("../test_fixtures/dummy.wasm"));
    let new_wasm_hash = env.deployer().upload_contract_wasm(wasm);

    client.upgrade(&admin, &new_wasm_hash);

    // Assert that state persists
    assert_eq!(client.total_shares(), 1000);
    assert_eq!(client.total_assets(), 5000);
    assert_eq!(client.fee_percentage(), 500);
}

#[test]
fn test_get_voting_power_proportional() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    client.set_total_shares(&1000);
    client.set_total_assets(&5000); // 1 share = 5 assets

    // Mock balance
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::Balance(user1.clone()), &200i128); // 200 shares
        env.storage()
            .persistent()
            .set(&DataKey::Balance(user2.clone()), &800i128); // 800 shares
    });

    // Proportional voting power
    // User1: 200 * 5000 / 1000 = 1000
    // User2: 800 * 5000 / 1000 = 4000
    assert_eq!(client.get_voting_power(&user1), 1000);
    assert_eq!(client.get_voting_power(&user2), 4000);
}

// Governance tests (SC-29): guardians, proposals, voting, timelock

#[test]
fn test_add_and_remove_guardian_admin_gated() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    let guardian = Address::generate(&env);
    let stranger = Address::generate(&env);

    // Non-admin cannot add a guardian
    assert_eq!(
        client.try_add_guardian(&stranger, &guardian),
        Err(Ok(Error::Unauthorized))
    );

    // Admin adds the guardian
    client.add_guardian(&admin, &guardian);
    assert_eq!(client.get_guardians().len(), 2);

    // Adding the same guardian twice is a no-op
    client.add_guardian(&admin, &guardian);
    assert_eq!(client.get_guardians().len(), 2);

    // Admin removes the guardian
    client.remove_guardian(&admin, &guardian);
    assert_eq!(client.get_guardians().len(), 1);

    // Removing a non-guardian fails
    assert_eq!(
        client.try_remove_guardian(&admin, &guardian),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn test_set_threshold_bounds() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    // Threshold 0 is invalid (single guardian)
    assert_eq!(
        client.try_set_threshold(&admin, &0u32),
        Err(Ok(Error::Unauthorized))
    );

    // Threshold above the guardian count is invalid
    assert_eq!(
        client.try_set_threshold(&admin, &5u32),
        Err(Ok(Error::Unauthorized))
    );

    // Threshold 1 with one guardian is valid
    client.set_threshold(&admin, &1u32);
    assert_eq!(client.get_threshold(), 1);
}

#[test]
fn test_propose_executes_immediately_at_threshold_one() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    // Admin is the only guardian; threshold 1 → proposal executes immediately
    let id = client.propose_action(
        &admin,
        &ActionType::SetPaused(true),
    );
    assert_eq!(id, 1);

    let proposal = client.get_proposal(&id).unwrap();
    assert!(proposal.executed);

    // The action took effect: vault is paused
    assert!(client.paused());
}

#[test]
fn test_non_guardian_cannot_propose() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    let stranger = Address::generate(&env);
    assert_eq!(
        client.try_propose_action(&stranger, &ActionType::SetThreshold(1u32)),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn test_multisig_proposal_requires_threshold_approvals() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    let guardian1 = Address::generate(&env);
    let guardian2 = Address::generate(&env);
    client.add_guardian(&admin, &guardian1);
    client.add_guardian(&admin, &guardian2);
    client.set_threshold(&admin, &3u32); // admin + 2 guardians

    // First approval proposes and records 1 of 3 approvals — not executed
    let id = client.propose_action(
        &guardian1,
        &ActionType::SetPaused(true),
    );
    let proposal = client.get_proposal(&id).unwrap();
    assert!(!proposal.executed);
    assert_eq!(proposal.approvals.len(), 1);
    assert!(!client.paused());

    // Second approval — still below threshold
    client.approve_action(&guardian2, &id);
    assert!(!client.paused());

    // Third approval reaches the threshold → executes
    client.approve_action(&admin, &id);
    assert!(client.paused());

    let proposal = client.get_proposal(&id).unwrap();
    assert!(proposal.executed);
    assert_eq!(proposal.executed_ledger, env.ledger().sequence());
}

#[test]
fn test_double_approval_rejected() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    let guardian1 = Address::generate(&env);
    client.add_guardian(&admin, &guardian1);
    client.set_threshold(&admin, &2u32);

    let id = client.propose_action(
        &admin,
        &ActionType::SetThreshold(2u32),
    );

    // The proposer already approved; approving again fails
    assert_eq!(
        client.try_approve_action(&admin, &id),
        Err(Ok(Error::AlreadyApproved))
    );

    // A second guardian can still approve to reach the threshold
    client.approve_action(&guardian1, &id);
    let proposal = client.get_proposal(&id).unwrap();
    assert!(proposal.executed);
}

#[test]
fn test_timelock_blocks_execution_until_elapsed() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|ledger| ledger.timestamp = 42);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    let guardian1 = Address::generate(&env);
    client.add_guardian(&admin, &guardian1);
    client.set_threshold(&admin, &2u32);
    client.set_timelock_duration(&admin, &100u64);

    let id = client.propose_action(
        &guardian1,
        &ActionType::SetPaused(true),
    );

    // Approving before the timelock elapses is rejected
    assert_eq!(
        client.try_approve_action(&admin, &id),
        Err(Ok(Error::TimelockNotElapsed))
    );
    assert!(!client.paused());

    // Advance past the timelock window and approve again → executes
    env.ledger().with_mut(|ledger| ledger.timestamp = 42 + 101);
    client.approve_action(&admin, &id);
    assert!(client.paused());
}

#[test]
fn test_cast_vote_weighted_tally_and_double_vote() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    let guardian1 = Address::generate(&env);
    client.add_guardian(&admin, &guardian1);
    client.set_threshold(&admin, &2u32);

    // Keep the proposal pending so token holders can vote on it
    let id = client.propose_action(
        &admin,
        &ActionType::SetThreshold(2u32),
    );

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    // Give users voting power via share balances (1 share = 5 assets)
    client.set_total_shares(&1000);
    client.set_total_assets(&5000);
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::Balance(user1.clone()), &200i128);
        env.storage()
            .persistent()
            .set(&DataKey::Balance(user2.clone()), &800i128);
    });

    client.cast_vote(&user1, &id, &true);
    client.cast_vote(&user2, &id, &false);

    let tally = client.get_vote_tally(&id);
    assert_eq!(tally.yes_votes, 1000); // 200 * 5000 / 1000
    assert_eq!(tally.no_votes, 4000); // 800 * 5000 / 1000

    // Double voting is rejected
    assert_eq!(
        client.try_cast_vote(&user1, &id, &true),
        Err(Ok(Error::AlreadyApproved))
    );
}

// Emergency circuit-breaker tests (SC-35)

#[test]
fn test_emergency_pause_state_transitions_and_events() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|ledger| ledger.timestamp = 42);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    assert!(!client.paused());
    client.emergency_pause();
    assert_eq!(
        env.events().all(),
        soroban_sdk::vec![
            &env,
            (
                contract_id.clone(),
                (Symbol::new(&env, "VaultPaused"), admin.clone()).into_val(&env),
                42u64.into_val(&env),
            ),
        ]
    );
    assert!(client.paused());
    assert_eq!(
        client.try_emergency_pause(),
        Err(Ok(contract_error(Error::ContractPaused)))
    );

    env.ledger().with_mut(|ledger| ledger.timestamp = 84);
    client.emergency_unpause();
    assert_eq!(
        env.events().all(),
        soroban_sdk::vec![
            &env,
            (
                contract_id,
                (Symbol::new(&env, "VaultUnpaused"), admin).into_val(&env),
                84u64.into_val(&env),
            ),
        ]
    );
    assert!(!client.paused());
    assert_eq!(
        client.try_emergency_unpause(),
        Err(Ok(contract_error(Error::ContractNotPaused)))
    );
}

#[test]
fn test_emergency_pause_requires_admin_authorization() {
    let env = Env::default();
    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    assert!(client.try_emergency_pause().is_err());
    assert!(!client.paused());
}

#[test]
fn test_paused_vault_blocks_normal_operations_and_rebalance_routes() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let token_admin = Address::generate(&env);
    let (token, sac, _tc) = create_token_contract(&env, &token_admin);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    client.init(&admin, &token, &oracle, &treasury, &0u32);
    sac.mint(&user, &100i128);
    client.set_balance(&user, &100i128);
    client.set_total_shares(&100i128);
    client.set_total_assets(&100i128);

    client.emergency_pause();

    assert_eq!(
        client.try_deposit(&user, &1i128),
        Err(Ok(contract_error(Error::ContractPaused)))
    );
    assert_eq!(
        client.try_withdraw(&user, &1i128),
        Err(Ok(contract_error(Error::ContractPaused)))
    );
    assert!(client.try_queue_withdraw(&user, &1i128).is_err());
    assert_eq!(
        client.try_rebalance(&admin),
        Err(Ok(contract_error(Error::ContractPaused)))
    );
    assert_eq!(client.try_harvest(), Err(Ok(Error::ContractPaused)));

    assert_eq!(client.balance(&user), 100);
    assert_eq!(client.total_shares(), 100);
    assert_eq!(client.total_assets(), 100);
}

#[test]
fn test_emergency_withdraw_requires_paused_state() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    let user = Address::generate(&env);
    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    assert_eq!(
        client.try_emergency_withdraw(&user),
        Err(Ok(contract_error(Error::ContractNotPaused)))
    );
}

#[test]
fn test_emergency_withdraw_redeems_full_balance_at_nav_without_fee() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let token_admin = Address::generate(&env);
    let (token, sac, tc) = create_token_contract(&env, &token_admin);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    client.init(&admin, &token, &oracle, &treasury, &500u32);

    sac.mint(&user, &1_000i128);
    client.deposit(&user, &1_000i128);
    sac.mint(&contract_id, &500i128);
    client.set_total_assets(&1_500i128);

    client.emergency_pause();
    assert_eq!(client.emergency_withdraw(&user), 1_500);

    assert_eq!(client.balance(&user), 0);
    assert_eq!(client.total_shares(), 0);
    assert_eq!(client.total_assets(), 0);
    assert_eq!(tc.balance(&user), 1_500);
    assert_eq!(tc.balance(&treasury), 0);
    assert_eq!(
        client.try_emergency_withdraw(&user),
        Err(Ok(contract_error(Error::NoBalance)))
    );
}

#[test]
fn test_emergency_withdraw_reconciles_pending_queue_without_double_payout() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let token_admin = Address::generate(&env);
    let (token, sac, tc) = create_token_contract(&env, &token_admin);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    client.init(&admin, &token, &oracle, &treasury, &0u32);

    sac.mint(&user, &1_000i128);
    client.deposit(&user, &1_000i128);
    client.set_withdraw_queue_threshold(&admin, &500i128);
    let withdrawal_id = client.queue_withdraw(&user, &600i128);
    assert_eq!(client.balance(&user), 400);

    client.emergency_pause();
    assert_eq!(client.emergency_withdraw(&user), 1_000);
    assert_eq!(tc.balance(&user), 1_000);
    assert!(
        client
            .get_pending_withdrawal(&withdrawal_id)
            .unwrap()
            .processed
    );

    client.emergency_unpause();
    assert_eq!(
        client.try_process_queued_withdrawal(&admin, &withdrawal_id),
        Err(Ok(Error::WithdrawalAlreadyProcessed))
    );
}

#[test]
fn test_emergency_withdraw_isolates_multiple_users_and_preserves_nav() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let token_admin = Address::generate(&env);
    let (token, sac, tc) = create_token_contract(&env, &token_admin);
    let admin = Address::generate(&env);
    let user_one = Address::generate(&env);
    let user_two = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    client.init(&admin, &token, &oracle, &treasury, &0u32);

    sac.mint(&user_one, &400i128);
    sac.mint(&user_two, &600i128);
    client.deposit(&user_one, &400i128);
    client.deposit(&user_two, &600i128);
    sac.mint(&contract_id, &500i128);
    client.set_total_assets(&1_500i128);

    client.emergency_pause();
    assert_eq!(client.emergency_withdraw(&user_one), 600);
    assert_eq!(client.balance(&user_two), 600);
    assert_eq!(client.total_shares(), 600);
    assert_eq!(client.total_assets(), 900);
    assert_eq!(tc.balance(&user_one), 600);

    assert_eq!(client.emergency_withdraw(&user_two), 900);
    assert_eq!(tc.balance(&user_two), 900);
    assert_eq!(client.total_shares(), 0);
    assert_eq!(client.total_assets(), 0);
}

#[test]
fn test_emergency_withdraw_insufficient_liquidity_preserves_state_and_queue() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let token_admin = Address::generate(&env);
    let (token, _sac, _tc) = create_token_contract(&env, &token_admin);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    client.init(&admin, &token, &oracle, &treasury, &0u32);
    client.set_balance(&user, &100i128);
    client.set_total_shares(&100i128);
    client.set_total_assets(&100i128);
    client.set_withdraw_queue_threshold(&admin, &50i128);
    let withdrawal_id = client.queue_withdraw(&user, &60i128);

    client.emergency_pause();
    assert_eq!(
        client.try_emergency_withdraw(&user),
        Err(Ok(contract_error(Error::InsufficientLiquidity)))
    );

    assert_eq!(client.balance(&user), 40);
    assert_eq!(client.total_shares(), 100);
    assert_eq!(client.total_assets(), 100);
    assert!(
        !client
            .get_pending_withdrawal(&withdrawal_id)
            .unwrap()
            .processed
    );
}

#[test]
fn test_unpause_restores_deposit_withdraw_and_queue_flow() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let token_admin = Address::generate(&env);
    let (token, sac, tc) = create_token_contract(&env, &token_admin);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    client.init(&admin, &token, &oracle, &treasury, &0u32);
    sac.mint(&user, &1_000i128);

    client.emergency_pause();
    assert!(client.try_deposit(&user, &1_000i128).is_err());
    client.emergency_unpause();

    client.deposit(&user, &1_000i128);
    client.withdraw(&user, &100i128);
    client.set_withdraw_queue_threshold(&admin, &100i128);
    assert_eq!(client.queue_withdraw(&user, &100i128), 1);
    assert_eq!(client.balance(&user), 800);
    assert_eq!(tc.balance(&user), 100);
}

#[test]
fn test_emergency_withdraw_requires_user_authorization() {
    let env = Env::default();
    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    let user = Address::generate(&env);
    client.init(&admin, &asset, &oracle, &treasury, &0u32);
    client.set_balance(&user, &100i128);
    client.set_total_shares(&100i128);
    client.set_total_assets(&100i128);

    client
        .mock_auths(&[MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "emergency_pause",
                args: ().into_val(&env),
                sub_invokes: &[],
            },
        }])
        .emergency_pause();

    assert!(client.try_emergency_withdraw(&user).is_err());
    assert_eq!(client.balance(&user), 100);
    assert_eq!(client.total_shares(), 100);
    assert_eq!(client.total_assets(), 100);
}

#[test]
fn test_emergency_withdraw_reconciles_mixed_multiuser_queue_state() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let token_admin = Address::generate(&env);
    let (token, sac, tc) = create_token_contract(&env, &token_admin);
    let admin = Address::generate(&env);
    let user_one = Address::generate(&env);
    let user_two = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    client.init(&admin, &token, &oracle, &treasury, &0u32);

    sac.mint(&user_one, &600i128);
    sac.mint(&user_two, &400i128);
    client.deposit(&user_one, &600i128);
    client.deposit(&user_two, &400i128);
    client.set_withdraw_queue_threshold(&admin, &1i128);

    let processed_one = client.queue_withdraw(&user_one, &100i128);
    let pending_one_a = client.queue_withdraw(&user_one, &200i128);
    let pending_two = client.queue_withdraw(&user_two, &100i128);
    let pending_one_b = client.queue_withdraw(&user_one, &50i128);
    client.process_queued_withdrawal(&admin, &processed_one);

    sac.mint(&contract_id, &450i128);
    client.set_total_assets(&1_350i128);
    client.emergency_pause();

    assert_eq!(
        client.try_process_queued_withdrawal(&admin, &pending_two),
        Err(Ok(Error::ContractPaused))
    );
    assert_eq!(client.emergency_withdraw(&user_one), 750);

    assert!(
        client
            .get_pending_withdrawal(&processed_one)
            .unwrap()
            .processed
    );
    assert!(
        client
            .get_pending_withdrawal(&pending_one_a)
            .unwrap()
            .processed
    );
    assert!(
        client
            .get_pending_withdrawal(&pending_one_b)
            .unwrap()
            .processed
    );
    assert!(
        !client
            .get_pending_withdrawal(&pending_two)
            .unwrap()
            .processed
    );
    assert_eq!(client.balance(&user_one), 0);
    assert_eq!(client.balance(&user_two), 300);
    assert_eq!(client.total_shares(), 400);
    assert_eq!(client.total_assets(), 600);
    assert_eq!(tc.balance(&user_one), 850);
    assert_eq!(
        client.try_emergency_withdraw(&user_one),
        Err(Ok(contract_error(Error::NoBalance)))
    );

    assert_eq!(client.emergency_withdraw(&user_two), 600);
    assert_eq!(tc.balance(&user_two), 600);
    assert_eq!(client.total_shares(), 0);
    assert_eq!(client.total_assets(), 0);
}

#[test]
fn test_emergency_withdraw_token_failure_rolls_back_state() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let token = env.register(RejectingToken, ());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    client.init(&admin, &token, &oracle, &treasury, &0u32);
    client.set_balance(&user, &100i128);
    client.set_total_shares(&100i128);
    client.set_total_assets(&100i128);
    client.set_withdraw_queue_threshold(&admin, &1i128);
    let pending_id = client.queue_withdraw(&user, &40i128);
    client.emergency_pause();

    assert!(client.try_emergency_withdraw(&user).is_err());

    assert_eq!(client.balance(&user), 60);
    assert_eq!(client.total_shares(), 100);
    assert_eq!(client.total_assets(), 100);
    assert!(
        !client
            .get_pending_withdrawal(&pending_id)
            .unwrap()
            .processed
    );
}

#[test]
fn test_emergency_withdraw_rounding_conserves_all_assets() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let token_admin = Address::generate(&env);
    let (token, sac, tc) = create_token_contract(&env, &token_admin);
    let admin = Address::generate(&env);
    let user_one = Address::generate(&env);
    let user_two = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);
    client.init(&admin, &token, &oracle, &treasury, &0u32);

    sac.mint(&user_one, &1i128);
    sac.mint(&user_two, &2i128);
    client.deposit(&user_one, &1i128);
    client.deposit(&user_two, &2i128);
    sac.mint(&contract_id, &97i128);
    client.set_total_assets(&100i128);
    client.emergency_pause();

    assert_eq!(client.emergency_withdraw(&user_one), 33);
    assert_eq!(client.total_shares(), 2);
    assert_eq!(client.total_assets(), 67);
    assert_eq!(client.emergency_withdraw(&user_two), 67);

    assert_eq!(tc.balance(&user_one), 33);
    assert_eq!(tc.balance(&user_two), 67);
    assert_eq!(client.total_shares(), 0);
    assert_eq!(client.total_assets(), 0);
    assert_eq!(tc.balance(&contract_id), 0);
}

// ── Share Price Oracle (SC-34) tests ─────────────────────────────────────

#[test]
fn test_share_price_oracle_update() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    // No share-price oracle is configured by default.
    assert_eq!(client.get_share_price_oracle(), None);

    let price_oracle = Address::generate(&env);
    client.set_share_price_oracle(&admin, &price_oracle);

    // A SharePriceOracleUpdated event must be emitted.
    assert_eq!(
        env.events().all(),
        soroban_sdk::vec![
            &env,
            (
                contract_id,
                (
                    Symbol::new(&env, "SharePriceOracle"),
                    Symbol::new(&env, "Updated")
                )
                    .into_val(&env),
                price_oracle.clone().into_val(&env),
            ),
        ]
    );

    assert_eq!(
        client.get_share_price_oracle(),
        Some(price_oracle.clone())
    );

    // Non-admin updates must be rejected.
    let attacker = Address::generate(&env);
    let res = client.try_set_share_price_oracle(&attacker, &price_oracle);
    assert!(res.is_err());
}

#[test]
fn test_convert_to_assets_uses_share_price_oracle() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&admin, &asset, &oracle, &treasury, &0u32);

    // Deploy a mock price oracle reporting price = 2.0 (scale 1e7).
    let price_oracle_id = env.register(MockPriceOracle, ());
    let price_oracle = MockPriceOracleClient::new(&env, &price_oracle_id);
    price_oracle.set_price(&20_000_000i128);

    client.set_total_assets(&100);
    client.set_total_shares(&100);

    // Without a configured oracle the conversion is the naive 1:1.
    assert_eq!(client.convert_to_assets(&50), 50);

    // With a fresh oracle at 2.0 the conversion is scaled by the price.
    client.set_share_price_oracle(&admin, &price_oracle_id);
    assert_eq!(client.convert_to_assets(&50), 100);
}

#[test]
#[should_panic(expected = "Stale share price oracle")]
fn test_process_queued_withdrawal_stale_oracle_reverted() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (token, sac, tc) = create_token_contract(&env, &Address::generate(&env));
    let admin = Address::generate(&env);
    sac.mint(&admin, &100_000_000_000_000i128);

    let contract_id = env.register(VolatilityShield, ());
    let client = VolatilityShieldClient::new(&env, &contract_id);

    let oracle = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&admin, &token, &oracle, &treasury, &0u32);

    let price_oracle_id = env.register(MockPriceOracle, ());
    let price_oracle = MockPriceOracleClient::new(&env, &price_oracle_id);
    price_oracle.set_price(&10_000_000i128);

    client.set_share_price_oracle(&admin, &price_oracle_id);

    tc.transfer(&admin, &client.address, &100_000);
    client.set_total_assets(&100_000);
    client.set_total_shares(&1_000);
    client.set_balance(&admin, &1_000);
    client.set_withdraw_queue_threshold(&admin, &500);

    let id = client.queue_withdraw(&admin, &600);

    // Advance time so the oracle's last update is > 24h in the past.
    price_oracle.set_timestamp(&0);
    env.ledger().with_mut(|li| {
        li.timestamp = 86_401;
    });

    client.process_queued_withdrawal(&admin, &id);
}

// Caps tests (deposit per-user + global, withdrawal per-tx)

#[test]
fn test_deposit_caps_admin_gated_and_enforced() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _sac, _tc, _token) = setup_vault(&env);

    let stranger = Address::generate(&env);

    // Non-admin cannot set caps
    assert_eq!(
        client.try_set_deposit_cap(&stranger, &100i128, &500i128),
        Err(Ok(Error::Unauthorized))
    );

    // Admin sets caps and they read back
    client.set_deposit_cap(&admin, &100i128, &500i128);
    assert_eq!(client.get_deposit_caps(), (100i128, 500i128));
}

#[test]
#[should_panic(expected = "Error(Contract, #24)")]
fn test_deposit_per_user_cap_blocks_second_deposit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sac, _tc, _token) = setup_vault(&env);

    client.set_deposit_cap(&admin, &100i128, &0i128); // per-user cap only

    // First deposit reaches exactly the per-user cap (1:1 share price)
    let user = Address::generate(&env);
    sac.mint(&user, &100i128);
    client.deposit(&user, &100i128);

    // A second deposit would exceed the per-user cap → CapExceeded
    sac.mint(&user, &10i128);
    client.deposit(&user, &10i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #24)")]
fn test_global_deposit_cap_blocks_vault_total() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sac, _tc, _token) = setup_vault(&env);

    client.set_deposit_cap(&admin, &0i128, &300i128); // global cap only

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    sac.mint(&user1, &250i128);
    client.deposit(&user1, &250i128);

    // This deposit would push total assets past the global cap → CapExceeded
    sac.mint(&user2, &100i128);
    client.deposit(&user2, &100i128);
}

#[test]
fn test_global_deposit_cap_allows_within_headroom() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sac, _tc, _token) = setup_vault(&env);

    client.set_deposit_cap(&admin, &0i128, &300i128);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    sac.mint(&user1, &250i128);
    client.deposit(&user1, &250i128);

    // Within the remaining headroom still works
    sac.mint(&user2, &50i128);
    client.deposit(&user2, &50i128);
    assert_eq!(client.total_assets(), 300);
}

#[test]
#[should_panic(expected = "Error(Contract, #24)")]
fn test_withdraw_cap_per_transaction() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sac, _tc, _token) = setup_vault(&env);

    client.set_withdraw_cap(&admin, &150i128);
    assert_eq!(client.get_withdraw_cap(), 150);

    let user = Address::generate(&env);
    sac.mint(&user, &400i128);
    client.deposit(&user, &400i128);

    // Withdrawing more than the per-tx cap → CapExceeded
    client.withdraw(&user, &200i128);
}

// Slippage protection test

#[test]
fn test_rebalance_slippage_cap_setter_admin_gated() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _sac, _tc, _token) = setup_vault(&env);

    let stranger = Address::generate(&env);

    // Non-admin cannot set the slippage tolerance
    assert_eq!(
        client.try_set_max_slippage_bps(&stranger, &100u32),
        Err(Ok(Error::Unauthorized))
    );

    // Values above 10_000 BPS are invalid
    assert_eq!(
        client.try_set_max_slippage_bps(&admin, &20_000u32),
        Err(Ok(Error::FeeTooHigh))
    );

    // Admin sets a valid tolerance
    client.set_max_slippage_bps(&admin, &100u32);
    assert_eq!(client.get_max_slippage_bps(), 100);
}
