#!/usr/bin/env bash

set -euo pipefail

STATUS=$(MODE=price npx tsx vault-clmm.ts)

echo "$STATUS"

if ! echo "$STATUS" | grep -q "In range: false"; then
    echo "Position is still in range. Stop."
    exit 1
fi

echo "===== COLLECT FEES ====="

MODE=collect npx tsx vault-clmm.ts
MODE=collect SEND=1 npx tsx vault-clmm.ts

echo "===== SIMULATE CLOSE ====="

RESULT=$(MODE=close MIN_0=0 MIN_1=0 \
npx tsx vault-clmm.ts)

echo "$RESULT"

AMOUNT_0=$(echo "$RESULT" | \
awk '/Token 0 received:/ {print $NF}' | tail -1)

AMOUNT_1=$(echo "$RESULT" | \
awk '/Token 1 received:/ {print $NF}' | tail -1)

[[ "$AMOUNT_0" =~ ^[0-9]+$ ]]
[[ "$AMOUNT_1" =~ ^[0-9]+$ ]]

MIN_0=$((AMOUNT_0 * 99 / 100))
MIN_1=$((AMOUNT_1 * 99 / 100))

echo "Expected token 0: $AMOUNT_0"
echo "Expected token 1: $AMOUNT_1"

echo "Minimum token 0: $MIN_0"
echo "Minimum token 1: $MIN_1"

echo "===== PROTECTED SIMULATION ====="

MODE=close MIN_0="$MIN_0" MIN_1="$MIN_1" \
npx tsx vault-clmm.ts

echo "===== CLOSE POSITION ====="

MODE=close MIN_0="$MIN_0" MIN_1="$MIN_1" \
SEND=1 npx tsx vault-clmm.ts

MODE=status npx tsx vault-clmm.ts
