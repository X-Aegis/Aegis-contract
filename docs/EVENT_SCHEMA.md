# Event Schema

This document details the standardized events emitted by the Aegis-contract for indexing purposes.

All event topics use PascalCase formatting.

## Deposit
Emitted when a user deposits assets into the vault.

**Topics:**
- `Deposit`
- `from`: Address

**Payload:** (Tuple)
1. `amount`: i128 (Assets deposited)
2. `shares_to_mint`: i128 (Shares minted to the user)
3. `new_total_assets`: i128 (Total assets in vault after deposit)
4. `new_total_shares`: i128 (Total shares in vault after deposit)
5. `share_price_at_time`: i128 (Scaled share price `total_assets * 1e7 / total_shares`)

---

## Withdraw
Emitted when a user withdraws assets from the vault.

**Topics:**
- `Withdraw`
- `from`: Address

**Payload:** (Tuple)
1. `shares`: i128 (Shares burned by the user)
2. `net_assets`: i128 (Assets returned to the user, after fees)
3. `fee`: i128 (Fee collected)
4. `new_total_assets`: i128 (Total assets in vault after withdrawal)
5. `new_total_shares`: i128 (Total shares in vault after withdrawal)
6. `share_price_at_time`: i128 (Scaled share price `total_assets * 1e7 / total_shares`)

---

## VaultSnapshot
Emitted at the end of state-altering operations (like `Rebalance` and `Harvest`) to provide indexers with an updated view of the vault's health.

**Topics:**
- `VaultSnapshot`

**Payload:** (Tuple)
1. `total_assets_after`: i128 (Total assets in the vault)
2. `total_shares_after`: i128 (Total shares minted)

---

## VaultPaused
Emitted after the admin activates the emergency circuit breaker.

**Topics:**
- `VaultPaused`
- `admin`: Address

**Payload:**
- `timestamp`: u64 (Ledger timestamp at which emergency mode was activated)

---

## VaultUnpaused
Emitted after the admin deactivates the emergency circuit breaker.

**Topics:**
- `VaultUnpaused`
- `admin`: Address

**Payload:**
- `timestamp`: u64 (Ledger timestamp at which normal operation resumed)

---

## EmergencyWithdraw
Emitted when a user redeems their complete economic position while the vault is paused. The position includes both freely held shares and that user's unprocessed queued-withdrawal shares.

**Topics:**
- `EmergencyWithdraw`
- `from`: Address

**Payload:** (Tuple)
1. `shares_redeemed`: i128 (All economic shares redeemed by the user)
2. `assets_withdrawn`: i128 (Underlying assets transferred without a withdrawal fee)
3. `new_total_assets`: i128 (Total managed assets after redemption)
4. `new_total_shares`: i128 (Total outstanding shares after redemption)
