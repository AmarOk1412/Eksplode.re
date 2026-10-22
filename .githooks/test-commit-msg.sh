#!/bin/sh
# Tests for the commit-msg hook. Run: sh .githooks/test-commit-msg.sh
dir=$(dirname "$0")
tmp=$(mktemp)
fail=0

check() {
    expected=$1
    subject=$2
    printf '%s\n\nbody\n' "$subject" > "$tmp"
    sh "$dir/commit-msg" "$tmp" 2>/dev/null
    got=$?
    if [ "$expected" = ok ] && [ "$got" -ne 0 ]; then
        echo "FAIL: expected accept: $subject"; fail=1
    elif [ "$expected" = reject ] && [ "$got" -eq 0 ]; then
        echo "FAIL: expected reject: $subject"; fail=1
    fi
}

check ok "core: add rng"
check ok "docs: update readme"
check reject "feature: add rng"
check reject "core add rng"
check reject "core:missing space"
check ok "core: $(printf 'a%.0s' $(seq 1 74))"
check reject "core: $(printf 'a%.0s' $(seq 1 75))"
check ok "Merge branch 'x'"

rm -f "$tmp"
if [ "$fail" -eq 0 ]; then echo "commit-msg hook tests passed"; fi
exit "$fail"
