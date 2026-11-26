#!/usr/bin/env bash

set -euo pipefail

unset SEND

STATUS=$(MODE=status npx tsx vault-clmm.ts)

echo "$STATUS"

if ! echo "$STATUS" | grep -q "Position active: true"; then
    echo "No active position"
    exit 1
fi

MODE=collect npx tsx vault-clmm.ts
MODE=collect SEND=1 npx tsx vault-clmm.ts

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

echo "Minimum Token 0: $MIN_0"
echo "Minimum Token 1: $MIN_1"

MODE=close MIN_0="$MIN_0" MIN_1="$MIN_1" \
npx tsx vault-clmm.ts

MODE=close MIN_0="$MIN_0" MIN_1="$MIN_1" \
SEND=1 npx tsx vault-clmm.ts

MODE=status npx tsx vault-clmm.ts
