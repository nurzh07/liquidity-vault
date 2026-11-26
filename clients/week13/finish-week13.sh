#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")"

unset SEND

echo "======================================"
echo " WEEK 13 — FINAL DEVNET VERIFICATION"
echo "======================================"

solana config set --url devnet

echo "===== 1. USER SECURITY ====="

npx tsx final-test.ts

echo "===== 2. ADMIN SECURITY ====="

bash test-admin.sh

echo "===== 3. CLOSE CLMM POSITION ====="

bash close-any.sh

echo "===== 4. TOKEN B GUARD ====="

npx tsx final-test.ts

echo "===== 5. CONVERT TOKEN B ====="

AMOUNT=$(npx tsx balance-b.ts | tail -1)

[[ "$AMOUNT" =~ ^[0-9]+$ ]]

if [ "$AMOUNT" != "0" ]; then

    MODE=swap \
    DIRECTION=b2a \
    AMOUNT_IN="$AMOUNT" \
    npx tsx vault-swap.ts

fi

REMAINING=$(npx tsx balance-b.ts | tail -1)

if [ "$REMAINING" != "0" ]; then
    echo "FAIL: Token B is not empty"
    exit 1
fi

echo "PASS: Token B fully converted"

echo "===== 6. WITHDRAW SIMULATION ====="

npx tsx final-test.ts

echo "===== 7. REAL WITHDRAW ====="

SEND=1 npx tsx final-test.ts

echo "===== 8. FINAL VAULT STATUS ====="

MODE=status npx tsx vault-clmm.ts

echo "===== 9. ANCHOR BUILD ====="

cd "$HOME/liquidity_vault"

anchor build

echo "======================================"
echo " ALL SCRIPT CHECKS COMPLETED"
echo "======================================"
