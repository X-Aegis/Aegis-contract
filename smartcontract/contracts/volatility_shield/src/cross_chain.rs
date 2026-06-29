//! Cross-chain messaging bridge abstraction.
//!
//! Provides the data structures and contract interface for emitting
//! cross-chain rebalance payloads. Bridge endpoint management is
//! admin-only; any caller (typically the oracle or rebalance flow)
//! can emit a `CrossChainRebalance` event once the endpoint is registered.
//!
//! ## Security model
//! - Bridge endpoints are added/removed by admin only.
//! - Emitting a cross-chain message publishes a Soroban event; the actual
//!   relay to the target chain is handled by an off-chain indexer/relayer
//!   that listens for these events.
//! - The payload is self-describing: target chain, destination contract,
//!   asset, amount, and a nonce for deduplication.

use soroban_sdk::{contracttype, Address, Bytes};

/// Identifies a target chain for cross-chain rebalance messages.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TargetChain {
    /// Ethereum mainnet (chain ID 1).
    Ethereum,
    /// Binance Smart Chain (chain ID 56).
    Bsc,
    /// Polygon PoS (chain ID 137).
    Polygon,
    /// Arbitrum One (chain ID 42161).
    Arbitrum,
    /// Optimism (chain ID 10).
    Optimism,
    /// A generic EVM chain identified by its chain ID.
    Generic(u64),
}

/// A registered cross-chain bridge endpoint.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeEndpoint {
    /// Unique identifier for this endpoint (auto-incremented).
    pub id: u32,
    /// The target chain for this bridge.
    pub chain: TargetChain,
    /// The contract address on the target chain that receives the message.
    pub destination_contract: Bytes,
    /// Human-readable label (e.g. "Wormhole ETH", "Axelar Polygon").
    pub label: soroban_sdk::Symbol,
    /// Whether this endpoint is currently enabled.
    pub enabled: bool,
}

/// Payload emitted in a `CrossChainRebalance` event.
///
/// Off-chain relayers index this event and forward the payload to the
/// target bridge for delivery to `destination_chain`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrossChainRebalancePayload {
    /// Monotonically increasing nonce for deduplication.
    pub nonce: u64,
    /// The source vault/contract emitting the rebalance instruction.
    pub source: Address,
    /// Which chain the funds should be sent to.
    pub destination_chain: u64,
    /// The contract or account on the destination chain to receive funds.
    pub destination_contract: Bytes,
    /// The asset (token) to be transferred cross-chain.
    pub asset: Address,
    /// Amount in the asset's base units (stroops for XLM).
    pub amount: i128,
    /// Ledger timestamp when the payload was created.
    pub timestamp: u64,
    /// Optional memo/reference for the target chain.
    pub memo: Bytes,
}
