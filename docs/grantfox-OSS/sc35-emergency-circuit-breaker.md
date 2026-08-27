# SC-35: Emergency Withdrawal Circuit Breaker

Issue: [#80 — Emergency withdrawal circuit breaker — admin-pause](https://github.com/X-Aegis/Aegis-contract/issues/80)

## Overview

This change adds an admin-controlled circuit breaker to the Volatility Shield vault. During an incident, the admin can pause normal asset-moving operations while users retain a dedicated path to recover their complete economic position.

## Implementation

- `DataKey::Paused` stores the circuit-breaker state and defaults to `false` for upgrade compatibility.
- `emergency_pause()` and `emergency_unpause()` require authorization from the stored admin and emit `VaultPaused` and `VaultUnpaused` events.
- Pause guards cover deposits, direct and queued withdrawals, queued-withdrawal processing, rebalancing, harvesting, flash-loan callbacks, and cross-chain rebalance emission.
- `emergency_withdraw()` requires user authorization and is available only while paused.
- Emergency redemption combines freely held shares with the user's unprocessed queued shares, settles them at the current vault NAV, charges no withdrawal fee, and marks matching queue entries as processed.
- Direct token liquidity is checked before accounting state is persisted. Insufficient liquidity and token-transfer failures revert without consuming balances or queue entries.

## Security properties

- Only the configured admin can change pause state.
- A user can redeem only their own position.
- Queued shares cannot be paid twice after an emergency redemption.
- Multi-user queue entries remain isolated.
- Failed redemptions preserve user balances, vault totals, and queue state through Soroban transaction atomicity.
- Full-redemption rounding preserves aggregate vault accounting.

## Verification

The implementation adds 13 targeted circuit-breaker tests and passes the complete 78-test workspace suite. It was also verified with strict Clippy checks and a release build for `wasm32-unknown-unknown`.

```text
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo build --target wasm32-unknown-unknown --release
```

Detailed accounting behavior, event schemas, operational sequencing, and the direct-liquidity limitation are documented in [`docs/EMERGENCY_CIRCUIT_BREAKER.md`](../EMERGENCY_CIRCUIT_BREAKER.md).
