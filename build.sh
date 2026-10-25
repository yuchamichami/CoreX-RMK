#!/bin/sh
# Build application-only firmware. Never accesses a connected keyboard.
set -eu
COREX_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
COREX_SIDE=${1:-right}
COREX_VARIANT=${2:-both}
case "$COREX_SIDE" in right|left|stock-left|all) ;; *) echo 'Usage: ./build.sh [right|left|stock-left|all] [s140v6|s140v7|both]' >&2; exit 2 ;; esac
case "$COREX_VARIANT" in s140v6|s140v7|both) ;; *) echo 'Choose s140v6, s140v7 or both' >&2; exit 2 ;; esac
# Do not inherit a board/configuration from another build invocation.
unset COREX_BOARD COREX_RAYTAC_APP_BASE KEYBOARD_TOML_PATH
for COREX_HALF in right left stock-left; do
    if [ "$COREX_SIDE" != all ] && [ "$COREX_SIDE" != "$COREX_HALF" ]; then continue; fi
    COREX_TARGET="${CARGO_TARGET_DIR:-$COREX_ROOT/build/target}/$COREX_HALF"
    if [ "$COREX_HALF" = stock-left ]; then
        COREX_BOARD=prototype CARGO_TARGET_DIR="$COREX_TARGET" "$COREX_ROOT/tools/build-board.sh" left
        continue
    fi
    case "$COREX_HALF" in right) COREX_BOARD=raytac-r-a14 ;; left) COREX_BOARD=raytac-l-rev1 ;; esac
    for COREX_SD in s140v6 s140v7; do
        if [ "$COREX_VARIANT" != both ] && [ "$COREX_VARIANT" != "$COREX_SD" ]; then continue; fi
        case "$COREX_SD" in s140v6) COREX_BASE=0x26000 ;; s140v7) COREX_BASE=0x27000 ;; esac
        COREX_BOARD="$COREX_BOARD" COREX_RAYTAC_APP_BASE="$COREX_BASE" CARGO_TARGET_DIR="$COREX_TARGET" \
            "$COREX_ROOT/tools/build-board.sh" "$COREX_HALF"
    done
done
