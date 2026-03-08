#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{testutils::Address as _, Address, Env};

fn create_token_contract<'a>(
    env: &Env,
    admin: &Address,
) -> (Address, StellarAssetClient<'a>, TokenClient<'a>) {
    let contract_id = env.register_stellar_asset_contract_v2(admin.clone());
    let address = contract_id.address();
    let stellar_asset_client = StellarAssetClient::new(env, &address);
    let token_client = TokenClient::new(env, &address);
    (address, stellar_asset_client, token_client)
}

#[test]
fn test_mock_strategy_init() {
    let env = Env::default();
    let contract_id = env.register_contract(None, MockStrategy);
    let client = MockStrategyClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token = Address::generate(&env);

    client.init(&admin, &token);

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_token(), token);
    assert_eq!(client.balance(), 0);
}

#[test]
#[should_panic(expected = "Already initialized")]
fn test_mock_strategy_already_initialized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, MockStrategy);
    let client = MockStrategyClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token = Address::generate(&env);

    client.init(&admin, &token);
    // Should panic
    client.init(&admin, &token);
}

#[test]
fn test_mock_strategy_deposit_and_withdraw() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, MockStrategy);
    let client = MockStrategyClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_id, stellar_asset_client, token_client) = create_token_contract(&env, &token_admin);

    client.init(&admin, &token_id);

    // Mint tokens to the mock strategy contract
    stellar_asset_client.mint(&contract_id, &1000);

    // Test deposit
    client.deposit(&100);
    assert_eq!(client.balance(), 100);

    // Test withdraw
    client.withdraw(&50);
    assert_eq!(client.balance(), 50);
}

#[test]
#[should_panic(expected = "deposit amount must be positive")]
fn test_mock_strategy_deposit_negative() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, MockStrategy);
    let client = MockStrategyClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_id, _, _) = create_token_contract(&env, &token_admin);

    client.init(&admin, &token_id);
    client.deposit(&-10);
}

#[test]
#[should_panic(expected = "withdraw amount must be positive")]
fn test_mock_strategy_withdraw_negative() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, MockStrategy);
    let client = MockStrategyClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_id, _, _) = create_token_contract(&env, &token_admin);

    client.init(&admin, &token_id);
    client.withdraw(&-10);
}

#[test]
#[should_panic(expected = "insufficient balance")]
fn test_mock_strategy_withdraw_insufficient() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, MockStrategy);
    let client = MockStrategyClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_id, _, _) = create_token_contract(&env, &token_admin);

    client.init(&admin, &token_id);
    client.withdraw(&100);
}

#[test]
fn test_mock_strategy_set_balance() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, MockStrategy);
    let client = MockStrategyClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_id, _, _) = create_token_contract(&env, &token_admin);

    client.init(&admin, &token_id);

    // Test setting balance
    client.set_balance(&500);
    assert_eq!(client.balance(), 500);

    client.set_balance(&1000);
    assert_eq!(client.balance(), 1000);
}

#[test]
#[should_panic(expected = "balance cannot be negative")]
fn test_mock_strategy_set_balance_negative() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, MockStrategy);
    let client = MockStrategyClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_id, _, _) = create_token_contract(&env, &token_admin);

    client.init(&admin, &token_id);
    client.set_balance(&-100);
}
