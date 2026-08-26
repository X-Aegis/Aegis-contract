# XHedge (Aegis) — Smart Contract Guide: Building with Soroban 🌟

Welcome to the comprehensive guide for building smart contracts on the Stellar network using **Soroban**. This guide is designed to help you understand the "why" and "how" of Stellar's ecosystem and get you shipping decentralized applications quickly.

## 1. The Stellar Vision 🌍

**What is Stellar?**
Stellar is a decentralized, open network created to move money and store value. Its primary mission is **financial inclusion**—connecting the world's financial systems to ensure that money can move as easily as email.

**Why was it built?**
- **Asset Issuance:** Stellar makes it incredibly easy to issue digital representations of real-world assets (fiat currencies, stocks, gold).
- **Speed & Cost:** Transactions settle in seconds (3-5s) and cost fractions of a cent ($0.00001).
- **The "Anchor" Model:** It connects banks, payment systems, and people, acting as a bridge between traditional finance (TradFi) and blockchain.

**Enter Soroban** 🧠
Soroban is the smart contract platform added to Stellar. While Stellar's base layer handles payments and asset issuance efficiently, **Soroban** enables Turing-complete programmability. It allows you to build DeFi protocols, DAOs, and complex logic that interact seamlessly with Stellar's existing assets.

---

## 2. Setting Up Your Environment 🛠️

Soroban contracts are written in **Rust** and compiled to **WebAssembly (Wasm)**.

### Prerequisites
1.  **Rust & Cargo:** The primary language and package manager.
    ```bash
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    rustup target add wasm32-unknown-unknown
    ```
2.  **Soroban CLI:** Your swiss-army knife for building and deploying.
    ```bash
    cargo install --locked soroban-cli
    ```
3.  **Freighter Wallet:** The recommended browser extension for Stellar apps.

### 2.1 Funding Your Wallet (The Faucet) 🚰

You cannot deploy contracts without testnet tokens (XLM).

1.  **Generate Identity:**
    ```bash
    soroban config identity generate alice
    ```
2.  **Fund Alice:**
    Use Friendbot to fund your new identity on Testnet:
    ```bash
    curl "https://friendbot.stellar.org/?addr=$(soroban config identity address alice)"
    ```
    *Alternatively, go to the [Stellar Laboratory](https://laboratory.stellar.org/#account-creator?network=test) to create and fund accounts via UI.*

---

## 3. Core Concepts & Architecture 🏗️

### A. The Host Environment
Soroban contracts run in a sandboxed "Host Environment". Unlike Ethereum where you have direct access to almost everything, Soroban restricts access to ensure scalability.
- **No Standard Lib:** You cannot use standard Rust libraries (`std`). You must use the `soroban-sdk` crate (`no_std`).
- **Host Functions:** You interact with the blockchain (storage, crypto, other contracts) via specific host functions provided by the SDK.

### B. Storage (State)
Soroban has a unique storage model. You don't just "declare variables". You specifically choose where data lives:
- **Temporary Storage:** Cheapest, deleted after a short time (good for oracle data).
- **Instance Storage:** Tied to the contract instance, lives as long as the contract does (good for admin keys).
- **Persistent Storage:** Expensive, permanent data (good for user balances).

### C. Authentication (Auth)
Forget `msg.sender`. Soroban uses a powerful **Auth Framework**.
- You don't ask "who called this?".
- You ask "**Does this address authorize this action?**"
- `address.require_auth()` prompts the user (wallet) to sign the transaction.

---

## 4. Hello World: Your First Contract 👋

Create a new project:
```bash
soroban contract init hello_world
cd hello_world
```

**`src/lib.rs`**:
```rust
#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, vec, Env, Symbol, Vec};

#[contract]
pub struct HelloContract;

#[contractimpl]
impl HelloContract {
    pub fn hello(env: Env, to: Symbol) -> Vec<Symbol> {
        vec![&env, symbol_short!("Hello"), to]
    }
}
```

### Key Takeaways:
- `#[contract]`: Marks the struct as a smart contract.
- `#[contractimpl]`: Where you define the public functions.
- `Env`: The environment object passed to every function, giving access to the blockchain.

---

## 5. Testing (The "Superpower") 🧪

Soroban allows you to run contracts **natively** on your machine without a local blockchain. It's incredibly fast.

**`src/test.rs`**:
```rust
#![cfg(test)]
use super::*;
use soroban_sdk::Env;

#[test]
fn test() {
    let env = Env::default();
    let contract_id = env.register_contract(None, HelloContract);
    let client = HelloContractClient::new(&env, &contract_id);

    let words = client.hello(&symbol_short!("Dev"));
    assert_eq!(words, vec![&env, symbol_short!("Hello"), symbol_short!("Dev")]);
}
```
Run it with `cargo test`.

---

## 6. Deployment & Interaction 🚀

1.  **Build:**
    ```bash
    soroban contract build
    ```
2.  **Deploy (Testnet):**
    ```bash
    soroban contract deploy \
        --wasm target/wasm32-unknown-unknown/release/hello_world.wasm \
        --source alice \
        --network testnet
    ```
3.  **Invoke:**
    ```bash
    soroban contract invoke \
        --id <CONTRACT_ID> \
        --source alice \
        --network testnet \
        -- \
        hello \
        --to Dev
    ```

## 7. Resources & Tools 📚
- **Stellar Laboratory:** Explore the network state.
- **Soroban Docs:** [developers.stellar.org/docs](https://developers.stellar.org/docs)
- **Rust Book:** Essential for mastering the language quirks.

---
*Happy Building! 🚀*
