# Security Documentation for Aegis-Contract

## Overview

Aegis-Contract is a multi-asset Soroban smart contract vault that accepts whitelisted stablecoins, deposits them into yield strategies, and mints vault shares to users. This document outlines the security model, threat assumptions, known limitations, and accepted risks.

**Protocol Type**: Yield vault with multisig governance  
**Chain**: Stellar Soroban  
**Languages**: Rust (Soroban SDK)  
**Version**: 1.0 (schema v1/v2 compatible)

---

## 1. Trust Model

### 1.1 Key Roles

| Role | Authority | Responsibilities |
|------|-----------|------------------|
| **Admin** | Contract initialization, all administrative functions | Contract initialization, strategy management, pause control, cap management, guardian management, WASM upgrades, migrations |
| **Oracle** | Trigger rebalancing (if not multisig-gated) | Monitor yield opportunities; propose rebalancing allocations |
| **Guardians** | Propose and approve multisig actions | Vote on critical governance decisions (pause, add strategy, rebalance) |
| **Treasury** | Recipient of withdrawal fees | No active authority; trusted not to be a burn address if fees are intended |
| **Strategy Contracts** | External yield providers | Implement deposit(), withdraw(), balance() correctly; not re-enter vault |
| **Users** | Deposit and withdraw their own funds | Self-signed authorization for deposits and withdrawals |

### 1.2 Trust Assumptions

**Admin Assumptions:**
- Admin address is non-compromised and controlled by a multisig or timelock.
- Admin will not rugpull the vault by draining strategies or setting malicious parameters.
- Admin will not call `upgrade()` with a malicious WASM without thorough testing.

**Oracle Assumptions:**
- Oracle provides accurate market price data (if used for rebalancing decisions externally).
- Oracle will not propose rebalancing to attacker-controlled strategies.

**Guardian Assumptions:**
- Guardians are honest custodians; at least `threshold` of them are non-compromised.
- Guardians will not collude to vote in malicious proposals.
- Guardian set will be rotated or updated if any guardian is suspected compromised.

**Strategy Assumptions:**
- Each strategy contract correctly implements the StrategyTrait interface.
- Strategy `deposit()`, `withdraw()`, and `balance()` methods are honest (no re-entry, no inflation).
- Strategies do not hold exploitable vulnerabilities that could drain vault funds.
- Strategies are not paused, frozen, or blocked from receiving/sending tokens.

**Token (Stablecoin) Assumptions:**
- Accepted stablecoins are non-rebasable, non-deflationary tokens (e.g., USDC, EURC).
- Stablecoins maintain a 1:1 peg to their underlying fiat currency (for practical purposes).
- Stablecoins will not be paused, blacklisted, or subject to regulatory freezes (risk assumed by users).

---

## 2. Threat Model

### 2.1 Assumed Threats (IN Scope)

#### Smart Contract Level

| Threat | Mitigation |
|--------|-----------|
| **Integer Overflow / Underflow** | Use checked_add, checked_sub, checked_mul; use I256 for large multiplications in share calculations |
| **Reentrancy via Strategy Calls** | Strategies are external contracts; assume non-malicious. For v2+, consider guard/mutex pattern if reentrancy is detected |
| **Slippage from Strategy Yield** | Rebalance includes post-call slippage checks; users subject to withdrawal fees (transparent in conversion math) |
| **Share Rounding Attacks** | Division truncates favorably to vault on deposit, to user on withdrawal. Minimum deposit enforcement (future enhancement) can prevent dust accumulation |
| **Pause Bypass** | Pause blocks deposit/withdraw only. View functions (harvest, rebalance if admin-called) can still proceed. Pause intended for user-facing ops only |
| **Double-Execution of Proposals** | Proposals marked `executed = true` after action; single execution expected per proposal |

#### Governance Level

| Threat | Mitigation |
|--------|-----------|
| **Multisig Threshold Bypass** | Threshold set to require N of M signatures; must reach threshold before execution. Guardians cannot unilaterally sign |
| **Expired Proposal Execution** | Proposals expire after 7 days; approve_multisig_action checks expiration before signing |
| **Guardian Exhaustion** | remove_guardian() panics if removal would break threshold. Must manually adjust threshold first |
| **Proposal Expiration Deadlock** | Expired proposals cannot reach threshold; admin may need to propose new action if first expires |

#### Access Control Level

| Threat | Mitigation |
|--------|-----------|
| **Unauthorized Admin Actions** | All admin functions require admin.require_auth(); cryptographic signature enforced |
| **User Fund Theft via Malicious Admin** | Multisig governance (init_multisig) can gate critical admin actions; timelock (propose_action / execute_action) adds delay |
| **Unauthorized Rebalancing** | rebalance() requires admin or oracle authorization; multisig gating available if threshold > 0 |

#### Economic Level

| Threat | Mitigation |
|--------|-----------|
| **Flash Loan / Arbitrage** | Shares are not flashloan-able (burned on withdrawal); slippage checks prevent certain arbitrage paths |
| **Deposit Front-Running** | Share minting uses current totals; deposits don't require slippage tolerance (direct conversion) |
| **Oracle Manipulation** | External oracle queries are not implemented in-contract; rebalancing decisions are off-chain or admin-driven |

### 2.2 Out-of-Scope Threats

The following threats are **assumed acceptable risks** and are NOT mitigated in this contract:

| Threat | Reason | User Responsibility |
|--------|--------|-------------------|
| **Stablecoin Depeg** | Contract assumes accepted stablecoins maintain peg. Depeg is a market risk, not a smart contract vulnerability. | Users choose which stablecoins to deposit; DYOR on peg stability |
| **Regulatory Blacklist / Seizure** | If a stablecoin issuer freezes accounts or blacklists addresses, contract cannot prevent it. | Users assume regulatory risk of chosen stablecoin |
| **Yield Strategy Insolvency** | If a strategy contract becomes insolvent or is hacked, vault funds in that strategy are at risk. | Admin and users should audit and monitor strategy contracts; rotate strategies periodically |
| **Ledger State Corruption** | Soroban ledger itself is assumed non-Byzantine. Corruption or censorship at the ledger layer is out of scope. | Assume Stellar network integrity |
| **Admin Key Compromise** | If admin's private key is stolen, attacker can drain the vault. | Admin must use hardware wallet, multisig, or timelock to mitigate |
| **Stellar Network Downtime** | If Soroban goes offline or experiences extended downtime, users cannot access funds. | Assume Stellar network SLA; no smart contract can fix network outages |

---

## 3. Known Limitations and Accepted Risks

### 3.1 Share Conversion Rounding

**Description:**  
When converting between assets and shares, division truncates (rounds down). This favors the vault on deposits and users on withdrawals:
- Deposit: `shares = amount * total_shares / total_assets` (rounds down → user loses dust)
- Withdraw: `assets = shares * total_assets / total_shares` (rounds down → user loses dust)

**Impact:**  
Users lose fractions of tokens on every deposit/withdraw cycle. The vault accumulates dust.

**Mitigation:**  
Use I256 arithmetic to prevent overflow; monitor accumulated dust; enforce minimum deposit amount (future enhancement).

**Accepted Risk:**  
Per-user dust is negligible for typical (>1 USDC) deposits. Vault accumulation is transparent and can be harvested as part of fees.

---

### 3.2 Per-User Deposit Cap Never Resets

**Description:**  
The `UserDeposited[address]` storage key accumulates deposits forever. It never resets, even if the user withdraws everything.

**Impact:**  
A user who deposits 1M USDC, then withdraws 1M USDC, still counts as having deposited 1M against their cap. A second deposit of 0.1M USDC would fail if the cap is 1M.

**Mitigation:**  
Document that the cap is a **lifetime cap**, not a per-period cap. If per-period resets are needed, implement in a future version with time-based reset logic.

**Accepted Risk:**  
Lifetime cap is a design choice and acceptable if documented. For recurring users, cap can be raised or removed by admin.

---

### 3.3 Withdrawal Lacks Slippage Control

**Description:**  
`withdraw()` does not support an optional `max_fee_bps` or `min_assets_out` parameter. Users are exposed to fees without a preview mechanism.

**Impact:**  
Users cannot atomically verify that fees are acceptable before committing to withdrawal. If fee % is changed between signing and block inclusion, user may receive less than expected.

**Mitigation:**  
Use `get_deposit_cap()` and `get_withdraw_cap()` view functions to preview cap state; calculate expected fees off-chain before signing.

**Accepted Risk:**  
Acceptable if users are educated to call view functions first. Future version can add optional slippage parameters.

---

### 3.4 Pause Does Not Block All State Changes

**Description:**  
`set_paused(true)` only blocks `deposit()` and `withdraw()`. It does NOT block:
- `rebalance()` (admin/oracle only)
- `harvest()` (admin only)
- `upgrade()` and `migrate()` (admin only)

**Impact:**  
When paused, users cannot exit the vault, but admin can still move funds between strategies or upgrade code. This creates an asymmetry.

**Mitigation:**  
Document pause intent: it is a **user-facing circuit breaker**, not a full freeze. For total freeze, use a separate `frozen` flag or multisig governance.

**Accepted Risk:**  
Acceptable if pause semantics are clearly documented. Most protocols pause user-facing ops only.

---

### 3.5 Guardian Removal Requires Threshold Adjustment

**Description:**  
`remove_guardian()` panics if the removal would cause guardians.len() < threshold. To remove a guardian, admin must first call `set_threshold(new_threshold)` with a lower value.

**Impact:**  
This adds operational friction; a multi-step process is required to remove a guardian.

**Mitigation:**  
Document the required workflow: call `set_threshold(threshold - 1)` before `remove_guardian()`. Future version can allow atomic removal with automatic threshold adjustment.

**Accepted Risk:**  
Acceptable operational friction; prevents accidental deadlock (threshold > guardians).

---

### 3.6 Multisig Proposal Expiration Hardcoded to 7 Days

**Description:**  
Proposals expire exactly 7 days (604,800 seconds) after creation, hardcoded in `propose_multisig_action()` (line 332).

**Impact:**  
Expiration is not configurable. If guardians are slow to sign, proposals expire and must be re-proposed.

**Mitigation:**  
Consider adding `set_proposal_ttl(duration)` admin function in a future version. For now, document the 7-day window to guardians.

**Accepted Risk:**  
Acceptable if 7 days is reasonable for the guardian set's signing time. Future enhancement can make configurable.

---

### 3.7 No Reentrancy Guard on Cross-Contract Calls

**Description:**  
Calls to strategy.deposit(), strategy.withdraw(), and token_client.transfer() are not protected by a reentrancy guard. If a strategy re-enters `rebalance()` or `deposit()`, the contract state could be corrupted.

**Impact:**  
Malicious strategy contract could re-enter during rebalance and manipulate state, potentially draining funds or inflating balances.

**Mitigation:**  
Assume strategy contracts are non-malicious and audited. In future versions, add reentrancy guards (e.g., a `locked` flag in instance storage).

**Accepted Risk:**  
Acceptable if strategies are whitelisted and audited. Recommend admin review of strategy code before adding.

---

### 3.8 Accepted Assets Cannot Be Removed

**Description:**  
`add_accepted_asset()` whitelists a stablecoin, but there is no `remove_accepted_asset()` function. Once added, an asset is permanent.

**Impact:**  
If a stablecoin is discovered to be deflationary or at risk of depeg, it cannot be unwhitelisted without a contract upgrade or migration.

**Mitigation:**  
Add `remove_accepted_asset()` admin function in a future version. For now, admin must use upgrade/migrate to remove a bad asset.

**Accepted Risk:**  
Acceptable if admin is diligent about vetting stablecoins before adding. Recommend pausing deposits of a bad asset as a workaround (via multisig or admin pause).

---

### 3.9 Version Migrations Are Strictly Sequential

**Description:**  
Migrations must be applied in order: v1→v2→v3→... No skipping or downgrading is allowed.

**Impact:**  
If a migration to v2 fails (e.g., due to a bug in the new WASM), the contract is stuck and cannot downgrade to v1. A new v2 must be deployed to fix.

**Mitigation:**  
Test all migrations thoroughly on testnet before deploying to mainnet. Consider snapshotting state before upgrade/migrate as an off-chain backup.

**Accepted Risk:**  
Acceptable if migrations are tested exhaustively. Recommend a staged rollout (testnet → stagenet → mainnet) for critical upgrades.

---

## 4. Storage and State Invariants

### 4.1 Critical Invariants

The following invariants MUST be maintained at all times. Violations indicate a bug or attack:

1. **Total Assets ≥ Sum of Per-Asset Balances**  
   `TotalAssets >= sum(AssetBalance[asset] for all assets)`  
   Ensures no accidental creation of assets.

2. **Total Shares = Sum of User Balances**  
   `TotalShares == sum(Balance[user] for all users)`  
   Ensures all minted shares are accounted for.

3. **Non-Negative Balances**  
   `Balance[user] >= 0` for all users; `AssetBalance[asset] >= 0` for all assets.  
   Enforced by checked_sub() panicking on underflow.

4. **Share Conversion Monotonicity**  
   `convert_to_shares(convert_to_assets(x)) >= x - 1` (due to rounding)  
   Ensures share math is not inverted or corrupted.

5. **Multisig Threshold Validity**  
   `Guardians.len() >= Threshold` always.  
   Enforced by remove_guardian() and set_threshold() checks.

6. **Version Only Increments**  
   `Version_new == Version_old + 1` after successful migrate().  
   Enforced by InvalidMigrationVersion error.

### 4.2 Storage Breakdown

**Instance Storage** (persists across upgrades, can be accessed by any callable):
```
Admin: Address
Asset: Address (primary stablecoin)
Oracle: Address
Treasury: Address
FeePercentage: u32 (basis points, e.g., 250 = 2.5%)
Token: Address (same as Asset)
Strategies: Vec<Address>
Paused: bool
TotalAssets: i128
TotalShares: i128
MaxDepositPerUser: i128
MaxTotalAssets: i128
MaxWithdrawPerTx: i128
TimelockDuration: u64
TimelockProposal: u64
Guardians: Vec<Address>
Threshold: u32
NextProposalId: u64
MaxStrategies: u32 (v2+ only)
AcceptedAssets: Vec<Address>
AssetBalance(Address): i128 (one key per asset)
```

**Persistent Storage** (per-address or per-ID, survives instance storage updates):
```
Balance(Address): i128 (user's share balance)
UserDeposited(Address): i128 (cumulative deposits)
Proposal(u64): Proposal struct
Signatures(u64): Vec<Address>
```

### 4.3 State Transitions

**Normal Flow:**
```
User calls deposit(amount) → Shares minted → Balance[user] increases → TotalShares increases
User calls withdraw(shares) → Shares burned → Balance[user] decreases → TotalShares decreases
Admin calls rebalance(allocations) → Strategies[].balance updated → TotalAssets unchanged (internal reallocation)
```

**Emergency Flow:**
```
Admin calls set_paused(true) → Paused = true → deposit/withdraw blocked
Admin calls propose_action() → TimelockProposal set → execute_action() after delay
```

**Governance Flow:**
```
Guardian calls propose_multisig_action() → Proposal created → NextProposalId++
Guardians call approve_multisig_action() → Signatures collected → Auto-execute on threshold
```

---

## 5. Access Control Matrix

| Function | Auth | Caller |
|----------|------|--------|
| `init()` | None | Anyone (once) |
| `deposit()` | `from.require_auth()` | User |
| `withdraw()` | `from.require_auth()` | User |
| `rebalance()` | `admin.require_auth()` OR `oracle.require_auth()` | Admin or Oracle |
| `add_strategy()` | `admin.require_auth()` | Admin |
| `harvest()` | `admin.require_auth()` | Admin |
| `set_paused()` | `admin.require_auth()` | Admin |
| `set_deposit_cap()` | `admin.require_auth()` | Admin |
| `set_withdraw_cap()` | `admin.require_auth()` | Admin |
| `upgrade()` | `admin.require_auth()` | Admin |
| `migrate()` | `admin.require_auth()` | Admin |
| `init_multisig()` | `admin.require_auth()` | Admin |
| `add_guardian()` | `admin.require_auth()` | Admin |
| `remove_guardian()` | `admin.require_auth()` | Admin |
| `set_threshold()` | `admin.require_auth()` | Admin |
| `propose_multisig_action()` | `creator.require_auth()` + guardian check | Guardian |
| `approve_multisig_action()` | `guardian.require_auth()` + guardian check | Guardian |
| All view functions | None | Anyone |

---

## 6. Compliance with Audit Standards

### 6.1 Code Quality

- [x] All public functions have NatSpec-style documentation (added in v1.1)
- [x] Cargo clippy runs with no warnings (fixed in v1.1)
- [x] Tests cover deposit, withdraw, and strategy interactions
- [x] Integer overflow checks via checked_* arithmetic
- [x] I256 used for large multiplications in share math

### 6.2 Security Best Practices

- [x] No hardcoded sensitive data (keys, secrets)
- [x] Role-based access control (admin, oracle, guardians, users)
- [x] Event logging for all state-changing operations
- [x] Reentrancy assumptions documented
- [x] Share conversion math uses safe types (I256)
- [x] Timelock and multisig governance available

### 6.3 Testing

Run the following to verify contract integrity:

```bash
# Compile and check for warnings
cargo clippy --all

# Run all tests
cargo test --all

# Build release WASM
cargo build --target wasm32-unknown-unknown --release

# Verify test snapshot
cargo test --all -- --nocapture | head -50
```

---

## 7. Incident Response

### 7.1 Pause the Vault

If an emergency is detected (e.g., strategy compromise):

1. **Via Admin** (if admin is trusted):
   ```
   Call set_paused(true) → blocks user deposits/withdrawals → keeps other ops available
   ```

2. **Via Multisig** (if governance is enabled):
   ```
   Guardian proposes SetPaused(true) → other guardians approve → auto-execute
   ```

### 7.2 Pause the Oracle

If rebalancing is causing losses:

1. **Stop calling rebalance()** from oracle/admin.
2. **Wait for yield accumulation** or manual rebalance decision.
3. **Review strategy performance** before resuming.

### 7.3 Remove a Compromised Strategy

1. **Do NOT call rebalance()** with that strategy.
2. **Set up a new strategy** with known-good contract.
3. **In next upgrade/migration**, remove the old strategy from list.

### 7.4 Recover from Failed Migration

If a migration fails or has a bug:

1. **Revert to previous WASM** using `upgrade()` if not yet called.
2. **Fix the bug** in the new WASM.
3. **Deploy fixed WASM** and retry `migrate()`.

---

## 8. Audit Checklist

- [x] All entry points documented with NatSpec comments
- [x] All clippy warnings resolved (0 warnings)
- [x] Threat model and trust assumptions documented
- [x] Known limitations and accepted risks listed
- [x] Storage invariants documented
- [x] Access control matrix provided
- [x] Integer overflow checks in place
- [x] Share conversion math uses I256 for large operations
- [x] Tests pass: `cargo test --all`
- [x] Code compiles to WASM: `cargo build --target wasm32-unknown-unknown --release`
- [x] No hardcoded secrets or sensitive data
- [x] Reentrancy assumptions documented
- [x] State transition flow documented
- [x] Pause mechanism is user-facing circuit breaker only
- [x] Multisig governance correctly implements signature thresholds
- [x] Proposal expiration prevents old proposals from executing
- [x] Migration sequencing enforced (v1→v2→v3)

---

## 9. Recommendations for Users

1. **Verify Accepted Assets**: Before depositing, check `get_accepted_assets()` to ensure your stablecoin is whitelisted.
2. **Monitor Strategies**: Review strategy contracts before vault launch; monitor their security.
3. **Use Multisig for Mainnet**: If deploying to mainnet, call `init_multisig()` with 2-of-3 or 3-of-5 guardians.
4. **Set Caps Appropriately**: Use `set_deposit_cap()` and `set_withdraw_cap()` to limit risk exposure.
5. **Regular Audits**: Audit strategy contracts and rebalancing decisions regularly.
6. **Guardians Rotation**: Rotate guardians periodically and monitor guardian activity.

---

## 10. Contact & Support

For security questions or vulnerability disclosures:

- **GitHub Issues**: [Aegis-Contract Issues](https://github.com/EbukaMoses/Aegis-contract)
- **Email**: (as specified in SECURITY policy)
- **Responsible Disclosure**: Please disclose vulnerabilities privately before public disclosure.

---

**Document Version**: 1.0  
**Date**: 2026-06-20  
**Contract Version**: 1.0 (Schema v1/v2 compatible)  
**Status**: Ready for formal security audit
