#!/bin/sh
# Checks that every acceptance scenario id of the web client (feature 002) and of
# customization, powers and audio (feature 003, ids start with f3_) appears in a
# test name or in the manual checklist of its feature.
#
# The checklists live in the planning workspace next to the game repository; set
# MANUAL_002 or MANUAL_003 to their paths. Without a checklist only test names count.
set -eu

cd "$(dirname "$0")/.."
MANUAL_002="${MANUAL_002:-${MANUAL:-../specs/002-web-client/checklists/manual-web.md}}"
MANUAL_003="${MANUAL_003:-../specs/003-customization-powers-audio/checklists/manual-003.md}"

ids_002="us1_as1 us1_as2 us1_as3 us1_as4 us1_as5 us1_as6
us2_as1 us2_as2 us2_as3 us2_as4
us3_as1 us3_as2 us3_as3 us3_as4
us4_as1 us4_as2 us4_as3"

# Scenario ids of feature 003 whose stage is implemented; extended stage by stage.
ids_003="f3_us1_as1 f3_us1_as2 f3_us1_as3 f3_us1_as4 f3_us1_as5 f3_us1_as6 f3_us1_as7
f3_us2_as1 f3_us2_as2 f3_us2_as3 f3_us2_as4 f3_us2_as5 f3_us2_as6 f3_us2_as7 f3_us2_as8 f3_us2_as9
f3_us3_as1 f3_us3_as2 f3_us3_as3 f3_us3_as4 f3_us3_as5 f3_us3_as6
f3_us4_as1 f3_us4_as2 f3_us4_as3 f3_us4_as4 f3_us4_as5 f3_us4_as6"

missing=0

in_tests() {
    # test names start a line with `fn` or `test("`; feature 002 ids must not match f3_ names
    grep -rhE "(fn|test\\(\")[^\"(]*$1" crates/*/tests crates/web/js-tests 2>/dev/null | grep -qv "f3_$1"
}

in_tests_f3() {
    grep -rqE "(fn|test\\(\")[^\"(]*$1" crates/*/tests crates/web/js-tests 2>/dev/null
}

in_manual() {
    [ -f "$2" ] || return 1
    spaced=$(echo "$1" | sed 's/^f3_//' | tr 'a-z_' 'A-Z ')
    grep -qiF "$1" "$2" || grep -qiF "$spaced" "$2"
}

# Feature 003 manual lines count only when ticked: `- [x] f3_us1_as3 ...`.
in_manual_ticked() {
    [ -f "$2" ] || return 1
    grep -qiE "^- \[x\] $1( |$)" "$2"
}

for id in $ids_002; do
    in_tests "$id" && continue
    in_manual "$id" "$MANUAL_002" && continue
    echo "missing: $id has no test name and no manual check"
    missing=$((missing + 1))
done

# F3_ONLY limits the check to some ids while a stage is still being built, e.g. F3_ONLY="f3_us1 f3_us2".
for id in $ids_003; do
    if [ -n "${F3_ONLY:-}" ]; then
        case " $F3_ONLY " in *" ${id%_as*} "*) ;; *) continue ;; esac
    fi
    in_tests_f3 "$id" && continue
    in_manual_ticked "$id" "$MANUAL_003" && continue
    echo "missing: $id has no test name and no manual check"
    missing=$((missing + 1))
done

if [ "$missing" -gt 0 ]; then
    echo "$missing scenario(s) are not covered" >&2
    exit 1
fi
echo "all scenarios covered"
