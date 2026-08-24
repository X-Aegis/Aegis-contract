# Emergency Circuit Breaker (SC-35)

This document defines the operational and accounting behavior of the Volatility Shield emergency circuit breaker.

## State and authorization

- `DataKey::Paused` is a global boolean stored in instance storage.
- New deployments initialize it to `false`.
- Reads default to `false` when the key is absent, preserving compatibility with contract instances deployed before SC-35.
- `emergency_pause()` and `emergency_unpause()` require authorization from the admin address stored by the vault.
- Repeating an already completed transition fails with `ContractPaused` or `ContractNotPaused`; no duplicate transition event is emitted.

## Operation matrix

| Entry point | Unpaused | Paused |
| --- | --- | --- |
| `deposit` | Allowed | Reverts with `ContractPaused` |
| `withdraw` | Allowed | Reverts with `ContractPaused` |
| `queue_withdraw` | Allowed | Reverts with `ContractPaused` |
| `process_queued_withdrawal` | Allowed | Reverts with `ContractPaused` |
| `rebalance` | Allowed | Reverts with `ContractPaused` |
| `harvest` | Allowed | Reverts with `ContractPaused` |
| `flash_loan_callback` | Allowed | Reverts with `ContractPaused` |
| `emit_cross_chain_rebalance` | Allowed | Reverts with `ContractPaused` |
| `emergency_withdraw` | Reverts with `ContractNotPaused` | Allowed |

Read-only functions and administrative configuration that do not execute a vault asset operation remain available.

## Emergency withdrawal accounting

`emergency_withdraw(from)` requires authorization from `from` and always redeems the user's complete economic position:

```text
shares_redeemed = available_share_balance + unprocessed_queued_shares
assets_withdrawn = floor(shares_redeemed * total_assets / total_shares)
```

This is the current proportional net asset value (NAV), which is the vault's share-to-asset face value. It is not a fixed one-share-to-one-asset conversion. No withdrawal fee, queue threshold, or partial amount is applied.

All unprocessed queue entries owned by the withdrawing user are marked processed in the same invocation. Their shares were already removed from the user's available balance when queued, but remained part of `total_shares`; including them once in the emergency redemption preserves the accounting invariant and prevents a later second payout.

The function verifies that the vault contract's current token balance can fund the calculated redemption. If liquidity is insufficient, it fails with `InsufficientLiquidity` before persisting any accounting or queue changes. Soroban transaction failure also rolls back the invocation atomically.

The current queue is stored by the pre-existing contract as one global `Map`. Emergency redemption therefore scans that bounded map to reconcile a user's queued shares. Adversarial budget tests found the scan remains below Soroban's execution budget for a map that fits within Stellar's 64 KiB ledger-entry limit. Redesigning the complete queue into per-withdrawal storage is a separate storage-migration concern, not part of SC-35.

## Events

- `VaultPaused(admin)` with the ledger timestamp as data.
- `VaultUnpaused(admin)` with the ledger timestamp as data.
- `EmergencyWithdraw(from)` with `(shares_redeemed, assets_withdrawn, new_total_assets, new_total_shares)` as data.

See [EVENT_SCHEMA.md](./EVENT_SCHEMA.md) for the indexer-facing schemas.

## Operational sequence

1. The admin invokes `emergency_pause()` and confirms the `VaultPaused` event.
2. Integrators query `paused()` and disable normal-operation transaction construction.
3. Each user invokes `emergency_withdraw(user_address)` with their own authorization.
4. If `InsufficientLiquidity` is returned, no user accounting was consumed. Operators must restore vault liquidity using the protocol's existing, separately governed strategy controls before the user retries; SC-35 does not add an unsafe strategy-recall interface.
5. After the incident is resolved and normal-operation invariants are verified, the admin invokes `emergency_unpause()` and confirms `VaultUnpaused`.

## Official technical references

- [Stellar contract authorization](https://developers.stellar.org/docs/build/guides/auth/contract-authorization)
- [Stellar storage selection](https://developers.stellar.org/docs/build/guides/storage/choosing-the-right-storage)
- [Stellar contract events](https://developers.stellar.org/docs/build/smart-contracts/example-contracts/events)
- [OpenZeppelin Stellar Pausable](https://docs.openzeppelin.com/stellar-contracts/utils/pausable)
- [OpenZeppelin Stellar Vault share accounting](https://docs.openzeppelin.com/stellar-contracts/tokens/vault/vault)
