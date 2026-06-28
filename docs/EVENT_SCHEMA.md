# Volatility Shield Event Schema

This document outlines the standardized events emitted by the `VolatilityShield` smart contract for off-chain indexers.

## Event Topics
All events use a standardized `symbol_short!` value as their first topic, and sometimes include additional topics like the caller address.

| Event Type | Topic 1 | Topic 2 | Payload (Tuple) | Description |
|---|---|---|---|---|
| **Deposit** | `Deposit` | `Address` (user) | `(i128, i128, i128, i128)`<br>`(amount, share_price, total_assets_after, total_shares_after)` | Emitted when a user deposits underlying assets. Includes the `share_price_at_time` for indexers. |
| **Withdraw** | `Withdraw` | `Address` (user) | `(i128, i128, i128, i128)`<br>`(shares, share_price, total_assets_after, total_shares_after)` | Emitted when a user withdraws shares. Includes the `share_price_at_time` for indexers. |
| **Strategy** | `Strategy` | `added` | `Address` (strategy) | Emitted when a new strategy is added by the admin. |
| **Harvest** | `Harvest` | - | `(i128, i128, i128)`<br>`(total_yield, total_assets_after, total_shares_after)` | Emitted when the vault harvests yield from all strategies. |
| **Snapshot** | `Snapshot` | - | `(i128, i128)`<br>`(total_assets_after, total_shares_after)` | Emitted after any state change that affects the overall vault valuation, such as a rebalance or harvest. |

## Payload Fields
- **amount**: The amount of underlying assets deposited or withdrawn.
- **shares**: The number of vault shares burned during withdrawal.
- **share_price**: Calculated as `total_assets_after * 10_000_000 / total_shares_after`. Defaults to `10_000_000` (1:1 ratio) if there are no shares.
- **total_assets_after**: The total assets managed by the vault after the transaction.
- **total_shares_after**: The total shares issued by the vault after the transaction.
- **total_yield**: The total yield harvested from strategies during the harvest event.
