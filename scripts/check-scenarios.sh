#!/bin/sh
# Checks that every acceptance scenario of feature 002 is covered: its id (for example
# us1_as3) must appear in a test name or in the manual checklist.
#
# The checklist lives in the planning workspace next to the game repository; set MANUAL to
# its path or leave the default. Without it only test names count.
set -eu

cd "$(dirname "$0")/.."
MANUAL="${MANUAL:-../specs/002-web-client/checklists/manual-web.md}"

ids="us1_as1 us1_as2 us1_as3 us1_as4 us1_as5 us1_as6
us2_as1 us2_as2 us2_as3 us2_as4
us3_as1 us3_as2 us3_as3 us3_as4
us4_as1 us4_as2 us4_as3"

missing=0
for id in $ids; do
    if grep -rqE "(fn|test\\(\")[^\"(]*${id}" crates/*/tests crates/web/js-tests 2>/dev/null; then
        continue
    fi
    if [ -f "$MANUAL" ] && grep -qi "$(echo "$id" | tr 'a-z_' 'A-Z ' | sed 's/US\([0-9]\) AS\([0-9]\)/US\1 AS\2/')" "$MANUAL"; then
        continue
    fi
    echo "missing: $id has no test name and no manual check"
    missing=$((missing + 1))
done

if [ "$missing" -gt 0 ]; then
    echo "$missing scenario(s) are not covered" >&2
    exit 1
fi
echo "all scenarios covered"
