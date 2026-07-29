gh pr edit 70 --repo X-Aegis/Aegis-contract --body "Closes #64

**Summary of Changes**
Added missing documentation to public methods and types across the vault contract to satisfy clippy rules and prepare for the upcoming security audit.

**What changed**
- Documented all public functions, structs, and enums in \`smartcontract/contracts/volatility_shield/src/lib.rs\`
- Ran \`cargo fmt\` to align formatting with the style guide

**Testing / Local Verification**
- \`cargo clippy --all-targets --all-features\` passes cleanly without warnings
- \`cargo build\` completes successfully
"

gh pr edit 71 --repo X-Aegis/Aegis-contract --body "Closes #63

**Summary of Changes**
Updated the core contract events to emit broader state context (total assets, total shares, fees) so off-chain indexers can track state transitions accurately without extra RPC calls.

**What changed**
- \`Deposit\` event now emits a tuple of \`(amount, shares_minted, new_total_assets, new_total_shares)\`
- \`Withdraw\` event now emits \`(shares, net_assets, fee, new_total_assets, new_total_shares)\`
- \`harvest\` event now includes the final total assets
- Added a new \`Rebalance\` event to broadcast the specific strategy allocations

**Testing / Local Verification**
- Verified syntax and logic via \`cargo build\` in \`smartcontract/contracts/volatility_shield\`
"

gh pr edit 72 --repo X-Aegis/Aegis-contract --body "Closes #66

**Summary of Changes**
Laid the groundwork for the future governance token integration by adding the storage key and admin controls to the vault.

**What changed**
- Added \`GovernanceToken\` to the \`DataKey\` enum
- Implemented \`set_governance_token\` (admin-only) to store the token address
- Implemented \`get_governance_token\` to read the token state

**Testing / Local Verification**
- Compiled successfully using \`cargo build\`
"
