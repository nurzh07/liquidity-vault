# Week 13 — Raydium CLMM Liquidity Vault

## Overview

The Liquidity Vault manages concentrated liquidity
positions in Raydium CLMM through Cross-Program
Invocations (CPI).

Network: Solana Devnet.

## Implemented functionality

- Open CLMM positions
- Collect trading fees
- Decrease liquidity
- Close CLMM positions
- Vault-owned position NFT
- Admin-only position management
- Slippage protection
- Independent trader wallet
- Price and tick monitoring
- Position rebalancing
- Deposit and withdrawal restrictions

## Technology stack

- Rust
- Anchor
- Solana
- Raydium CLMM
- SPL Token
- TypeScript
- Raydium SDK V2

## Program addresses

Vault program:
C9pH3cWmgQihD2xPQgWxsb7QyWkNpXQNPxsCBkVJrQM1

Vault PDA:
FpqwK4okMScfCTKEhtWdiyLLTYyv3JuW4bFWerKUQTVo

CLMM position PDA:
AbCrC5JcDN3xbLhsmQyu6296HZFBJRuep3UEoaFrSWUQ

Raydium CLMM program:
DRayAUgENGQBKVaX8owNhgzkEDyoHTGVEGHVJT1E9pfH

Raydium pool:
97pm3KyTiewPK1e9ho64madehSy6Xz7rqHAj2PKPZwr4

## Test results

### Initial position

Lower tick: -518
Upper tick: 483

Collected fees:

- Token A: 0.000007
- Token B: 0.000003

Returned after closing:

- Token A: 46.674867
- Token B: 48.953726

### Rebalancing test

Previous position:

- Lower tick: -311
- Upper tick: 290

Independent swaps moved the current tick to -318,
outside the previous position range.

The old position was closed and a new position
was opened around the updated market price.

New position:

- Lower tick: -831
- Upper tick: 170
- Current tick: -318
- Pool price: 0.9687152336723345
- Liquidity: 1179303895
- Position active: true
- In range: true

Additional confirmed trading fees:

- Token A: 0.003304

## Confirmed transactions

Collect fees:

https://explorer.solana.com/tx/3ynTbSQMpoAahQhL3jntY9eEWKyAqTn9gv7J3VCjda8y4pWtAnVj76jEsvBDTDEPXuiHLaT7qV5vRGZATbLnH5pe?cluster=devnet

Close position:

https://explorer.solana.com/tx/4j5mH16PdpwjVg7qsfLVYyYipWb9XctMFJePZj6bRG9x5zZ9fpaHVAenosUYCYjDCM5VYYzDMYvNTd9CJt5yqUik?cluster=devnet

Collect fees after independent swaps:

https://explorer.solana.com/tx/4HxFJCqn6Yokbxbf698SkRHipmvT9fzyZiAeimizexkpeCAZRVYztiTWDFGpuGWFYTcF4uXyJgXTqEFrDCG4nuQk?cluster=devnet

Open rebalanced position:

https://explorer.solana.com/tx/3RLyUj137u4LACTQZgQYzC8vGo7rR5qtkYH4f2vwrPU6F6sxCCr8Se57QDsowS2WyRhxUA5bSHybjsESfiGG24Df?cluster=devnet

## Running the client

Build the Anchor program from the repository root
before running the client.

Install dependencies:

    npm ci

Check position:

    MODE=status npx tsx vault-clmm.ts

Check price:

    MODE=price npx tsx vault-clmm.ts

Simulate opening:

    MODE=open RANGE_PERCENT=5 MAX_A=30000000 MAX_B=30000000 npx tsx vault-clmm.ts

Simulate fee collection:

    MODE=collect npx tsx vault-clmm.ts

Close an out-of-range position:

    ./close-safe.sh

## Security and limitations

- Admin authorization is required.
- The position NFT belongs to the Vault PDA.
- Private keys are stored outside the repository.
- Deposits and withdrawals are restricted while
  liquidity is deployed or Token B remains.
- Before user withdrawals, the admin must close
  the position and convert Token B back to Token A.
- Restriction regression tests remain pending.
- This project is educational and not audited
  for production use.

## Conclusion

The core CLMM position lifecycle and rebalancing
workflow were successfully demonstrated on Devnet.

## Final Devnet verification

```text
PASS: Deposit blocked during CLMM
PASS: Withdraw blocked during CLMM
PASS: Unauthorized collect rejected
PASS: Unauthorized close rejected
Transaction: 2jALW4MCjBCAucaiCUYLZFgfqQTyygcdgdPAPL3iGh3daRKcabcPv5teVsnQn78a9qsSKcSbgi7jS38xaWH3onr
Transaction: 3hBmCHNH6aTXh38QJ4wXcAm2Bvt9jMEDro5m2QgAZsYjaVpPNQLHNvUh1z8HfNHpVxcbpcam452vQEsYoeTRvUzA
PASS: Deposit blocked while Token B exists
PASS: Withdraw blocked while Token B exists
PASS: Token B fully converted
PASS: Withdraw simulation
PASS: Withdraw simulation
PASS: Real withdrawal
Transaction: rGwdDHiFn1PpMdhvS1p8zYVUdVgf1aYunZDew7T1BdV7dt4D4tE9LMkDrMjL6anUtNL2bgnr4jdizFf64P77wct
 ALL SCRIPT CHECKS COMPLETED
```
