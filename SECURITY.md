# Security Policy & Threat Model

## Overview
Aegis-contract is designed as a volatility shielding protocol on the Stellar network (via Soroban). This document outlines our threat model, trust assumptions, and accepted risks.

## Trust Assumptions
1. **Admins:** The assigned admin is assumed to be honest and competent. The admin role controls critical configurations such as updating the protocol fees and emergency halting.
2. **Soroban Network:** The underlying Stellar consensus and Soroban execution environment are assumed to operate correctly and without compromise.
3. **Oracles:** The protocol relies on integrated oracle endpoints for accurate price feeds. It is assumed that these endpoints provide timely and unmanipulated data.

## Threat Model
1. **Front-running / MEV:** Potential vulnerabilities around delayed oracle updates.
   - *Mitigation:* We employ staleness checks and rely on high-frequency oracle updates.
2. **Flash Loan Attacks:** Manipulation of pool reserves within a single transaction.
   - *Mitigation:* Internal accounting mechanisms and snapshot validations prevent artificial inflation of share prices.

## Known Limitations and Accepted Risks
- **Oracle Dependency:** In the event of an extended oracle outage, rebalance functions will halt, potentially exposing the vault to unhedged volatility. This is an accepted risk mitigated by emergency circuit breakers.
- **Liquidity Constraints:** In extreme market conditions, the underlying swap venues may lack sufficient liquidity to execute rebalances seamlessly, resulting in higher slippage.

## Reporting Vulnerabilities
If you discover a potential vulnerability, please do NOT disclose it publicly. Send a detailed report to our security team via the designated secure channel.
