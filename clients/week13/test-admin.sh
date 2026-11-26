#!/usr/bin/env bash

set -euo pipefail

unset SEND

for MODE in collect close; do

    echo "===== TEST $MODE ====="

    if OUTPUT=$(MODE="$MODE" \
        MIN_0=0 MIN_1=0 \
        npx tsx unauthorized-test.ts 2>&1); then

        echo "$OUTPUT"
        echo "FAIL: Unauthorized instruction accepted"
        exit 1
    fi

    echo "$OUTPUT"

    if echo "$OUTPUT" | grep -q "NotAdmin"; then
        echo "PASS: Unauthorized $MODE rejected"
    else
        echo "FAIL: Unexpected error"
        exit 1
    fi

done

echo "ADMIN SECURITY TESTS PASSED"
