#!/bin/sh
# Checks that every acceptance scenario id of the web client (feature 002) and of
# customization, powers and audio (feature 003, ids start with f3_) and of game
# modes and room settings (feature 004, ids start with f4_) appears in a test
# name or in the manual checklist of its feature.
#
# The checklists live in the planning workspace next to the game repository; set
# MANUAL_002, MANUAL_003 or MANUAL_004 to their paths. Without a checklist only test names count.
set -eu

cd "$(dirname "$0")/.."
MANUAL_002="${MANUAL_002:-${MANUAL:-../specs/002-web-client/checklists/manual-web.md}}"
MANUAL_003="${MANUAL_003:-../specs/003-customization-powers-audio/checklists/manual-003.md}"
MANUAL_004="${MANUAL_004:-../specs/004-game-modes/checklists/manual-004.md}"

ids_002="us1_as1 us1_as2 us1_as3 us1_as4 us1_as5 us1_as6
us2_as1 us2_as2 us2_as3 us2_as4
us3_as1 us3_as2 us3_as3 us3_as4
us4_as1 us4_as2 us4_as3"

# Scenario ids of feature 003 whose stage is implemented; extended stage by stage.
ids_003="f3_us1_as1 f3_us1_as2 f3_us1_as3 f3_us1_as4 f3_us1_as5 f3_us1_as6 f3_us1_as7 f3_us1_as8 f3_us1_as9
f3_us2_as1 f3_us2_as2 f3_us2_as3 f3_us2_as4 f3_us2_as5 f3_us2_as6 f3_us2_as7 f3_us2_as8 f3_us2_as9 f3_us2_as10 f3_us2_as11 f3_us2_as12
f3_us3_as1 f3_us3_as2 f3_us3_as3 f3_us3_as4 f3_us3_as5 f3_us3_as6
f3_us4_as1 f3_us4_as2 f3_us4_as3 f3_us4_as4 f3_us4_as5 f3_us4_as6"

# Scenario ids of feature 004 (game modes and room settings).
ids_004="f4_us1_as1 f4_us1_as2 f4_us1_as3 f4_us1_as4 f4_us1_as5 f4_us1_as6 f4_us1_as7 f4_us1_as8
f4_us2_as1 f4_us2_as2 f4_us2_as3 f4_us2_as4 f4_us2_as5 f4_us2_as6 f4_us2_as7
f4_us3_as1 f4_us3_as2 f4_us3_as3 f4_us3_as4 f4_us3_as5 f4_us3_as6
f4_us4_as1 f4_us4_as2 f4_us4_as3 f4_us4_as4 f4_us4_as5 f4_us4_as6 f4_us4_as7 f4_us4_as8 f4_us4_as9"

ids_013="f13_us1_as1 f13_us1_as2 f13_us1_as3 f13_us1_as4 f13_us1_as5 f13_us1_as6
f13_us2_as1 f13_us2_as2 f13_us2_as3 f13_us2_as4 f13_us2_as5 f13_us2_as6 f13_us2_as7
f13_us3_as1 f13_us3_as2 f13_us3_as3 f13_us3_as4 f13_us3_as5 f13_us3_as6 f13_us3_as7
f13_us4_as1 f13_us4_as2 f13_us4_as3 f13_us4_as4 f13_us4_as5 f13_us4_as6"

for selector in ${F13_ONLY:-}; do
    found=0
    for id in $ids_013; do
        case "$selector" in "$id" | "${id%_as*}") found=1 ;; esac
    done
    if [ "$found" -eq 0 ]; then
        echo "unknown feature-013 selector: $selector" >&2
        exit 1
    fi
done

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

# F3_ONLY limits the check to some stories or ids while a stage is still being built, e.g.
# F3_ONLY="f3_us1 f3_us2_as1".
for id in $ids_003; do
    if [ -n "${F3_ONLY:-}" ]; then
        case " $F3_ONLY " in *" ${id%_as*} "* | *" $id "*) ;; *) continue ;; esac
    fi
    in_tests_f3 "$id" && continue
    in_manual_ticked "$id" "$MANUAL_003" && continue
    echo "missing: $id has no test name and no manual check"
    missing=$((missing + 1))
done

# F4_ONLY limits the feature 004 check while a stage is still being built, e.g.
# F4_ONLY="f4_us1 f4_us2_as1"; without it every id must be covered.
for id in $ids_004; do
    if [ -n "${F4_ONLY:-}" ]; then
        case " $F4_ONLY " in *" ${id%_as*} "* | *" $id "*) ;; *) continue ;; esac
    fi
    in_tests_f3 "$id" && continue
    in_manual_ticked "$id" "$MANUAL_004" && continue
    echo "missing: $id has no test name and no manual check"
    missing=$((missing + 1))
done

registered_f13=$(cargo test -p eksplode-re-core --test round_end -- --list &&
    cargo test -p eksplode-re-bots --test behaviour -- --list &&
    cargo test -p eksplode-re-web --test mixed -- --list)
for id in $ids_013; do
    if [ -n "${F13_ONLY:-}" ]; then
        case " $F13_ONLY " in *" ${id%_as*} "* | *" $id "*) ;; *) continue ;; esac
    fi
    if ! printf '%s\n' "$registered_f13" | grep -qE "^${id}_[^:]*: test$"; then
        echo "missing: $id has zero registered automated tests" >&2
        missing=$((missing + 1))
    fi
done

ids_014="f14_us1_as1_ f14_us1_as2_ f14_us1_as3_ f14_us1_as4_ f14_us1_as5_
f14_us2_as1_ f14_us2_as2_ f14_us2_as3_ f14_us2_as4_ f14_us2_as5_ f14_us2_as6_
f14_us3_as1_ f14_us3_as2_ f14_us3_as3_ f14_us3_as4_ f14_us3_as5_ f14_us3_as6_
f14_us4_as1_ f14_us4_as2_ f14_us4_as3_"
registered_f14=$(cargo test -p eksplode-re-web-render --test lobby --test ffi -- --list &&
    cargo test -p eksplode-re-web --test http -- --list &&
    node --test --test-reporter=tap --test-name-pattern='^f14_' \
        crates/web/js-tests/app.test.mjs crates/web/js-tests/lobby.test.mjs \
        crates/web/js-tests/glue.test.mjs crates/web/js-tests/nav-focus.test.mjs)
for id in $ids_014; do
    if ! printf '%s\n' "$registered_f14" | grep -qE "^${id}[^:]*: test$|^ok [0-9]+ - ${id}"; then
        echo "missing: $id has zero registered automated tests" >&2
        missing=$((missing + 1))
    fi
done

if [ "$missing" -gt 0 ]; then
    echo "$missing scenario(s) are not covered" >&2
    exit 1
fi
echo "all scenarios covered"
