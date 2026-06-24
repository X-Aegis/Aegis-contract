//! Flash-loan support for rebalancing (SC-32 / #55).
//!
//! Soroban forbids contract re-entry, so flash loans are **provider-initiated**:
//! an admin-authorized transaction calls a whitelisted provider, the provider
//! lends to the vault and then calls the vault's
//! [`flash_loan_callback`](crate::VolatilityShield::flash_loan_callback)
//! (the vault appears only once on the call stack). The vault uses the borrowed
//! liquidity for rebalancing and repays `amount + fee` with a token transfer —
//! never a call back into the provider — so it is never re-entered. The
//! provider verifies repayment after the callback returns; if repayment is
//! short it reverts, unwinding the whole transaction (including the lend).

use soroban_sdk::{contractclient, Address, Env};

/// Callback the vault exposes to an active flash loan, implemented by
/// `VolatilityShield::flash_loan_callback`. A provider invokes it after
/// transferring the borrowed `amount` of `token` to the vault, passing its own
/// address as `initiator` so the vault can verify it is whitelisted and repay
/// it `amount + fee`.
#[allow(dead_code)] // the generated `FlashLoanReceiverClient` is used by providers/tests
#[contractclient(name = "FlashLoanReceiverClient")]
pub trait FlashLoanReceiver {
    fn flash_loan_callback(env: Env, token: Address, amount: i128, fee: i128, initiator: Address);
}
