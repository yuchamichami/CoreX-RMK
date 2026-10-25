#!/bin/sh
# Build the paired firmware without changing its persisted-settings schema.
set -eu
COREX_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
COREX_SIDE=${1:-}
case "$COREX_SIDE" in right|left) ;; *) printf 'Use ./build.sh [right|left|stock-left|all] [s140v6|s140v7|both]\n' >&2; exit 2 ;; esac
COREX_BOARD=${COREX_BOARD:-prototype}
case "$COREX_BOARD" in prototype|raytac-r-a14|raytac-l-rev1) ;; *) printf 'Unknown CoreX board\n' >&2; exit 2 ;; esac
if [ "$COREX_BOARD" != prototype ]; then
    case "$COREX_BOARD" in
        raytac-r-a14) COREX_EXPECTED_SIDE=right; COREX_MODEL=Right ;;
        raytac-l-rev1) COREX_EXPECTED_SIDE=left; COREX_MODEL=Left ;;
    esac
    [ "$COREX_SIDE" = "$COREX_EXPECTED_SIDE" ] || { printf 'Raytac board and target side do not match\n' >&2; exit 2; }
    case "${COREX_RAYTAC_APP_BASE:-}" in
        0x26000) COREX_SOFTDEVICE=s140v6; COREX_UF2_TARGET=raytac-$COREX_SIDE-26000 ;;
        0x27000) COREX_SOFTDEVICE=s140v7; COREX_UF2_TARGET=raytac-$COREX_SIDE-27000 ;;
        *) printf 'Use ./build.sh right [s140v6|s140v7|both]\n' >&2; exit 2 ;;
    esac
    KEYBOARD_TOML_PATH="$COREX_ROOT/source/corex-rmk-pair/$COREX_SIDE/keyboard-raytac.toml"
else
    [ "$COREX_SIDE" = left ] || { printf 'The prototype configuration is only supported for the stock Cornix left. Use ./build.sh right for CoreX right.\n' >&2; exit 2; }
    unset COREX_RAYTAC_APP_BASE
    KEYBOARD_TOML_PATH="$COREX_ROOT/source/corex-rmk-pair/left/keyboard.toml"
fi
export KEYBOARD_TOML_PATH
COREX_RUSTC=$(rustup which --toolchain 1.95.0 rustc)
COREX_RUST_BIN=$(dirname "$COREX_RUSTC")
COREX_BUILD_REAL_GIT=$(command -v git)
export COREX_BUILD_REAL_GIT
export LZMA_API_STATIC=1
export PATH="$COREX_ROOT/tools/git-metadata:$COREX_RUST_BIN:$HOME/.cargo/bin:$PATH"
COREX_SYSROOT=$(rustc --print sysroot)
COREX_HOST=$(rustc -vV | sed -n 's/^host: //p')
COREX_OBJCOPY="$COREX_SYSROOT/lib/rustlib/$COREX_HOST/bin/llvm-objcopy"
if [ ! -x "$COREX_OBJCOPY" ]; then
    printf 'Missing llvm-objcopy. Run: rustup component add llvm-tools-preview --toolchain 1.95.0\n' >&2
    exit 1
fi
command -v flip-link >/dev/null || { printf 'Missing flip-link. Run: cargo install flip-link --version 0.1.12 --locked\n' >&2; exit 1; }
command -v python3 >/dev/null
python3 "$COREX_ROOT/tools/generate_gestures.py" --check
COREX_FIRMWARE_VERSION=$(cat "$COREX_ROOT/source/corex-rmk-pair/VERSION")
export COREX_FIRMWARE_VERSION
CARGO_TARGET_DIR=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).expanduser().resolve())' "${CARGO_TARGET_DIR:-$COREX_ROOT/build/target}")
export CARGO_TARGET_DIR
# Rust's panic locations can otherwise embed the builder's private absolute paths
# into the flash image itself. Encoded flags also support clone paths with spaces.
CARGO_ENCODED_RUSTFLAGS=$(python3 - "$HOME" "$COREX_ROOT" "$CARGO_TARGET_DIR" "${CARGO_HOME:-$HOME/.cargo}" "${RUSTUP_HOME:-$HOME/.rustup}" <<'PY'
import os, shlex, sys
flags = os.environ.get('CARGO_ENCODED_RUSTFLAGS')
flags = flags.split('\x1f') if flags else shlex.split(os.environ.get('RUSTFLAGS', ''))
destinations = ['/build-home', 'corex-rmk-firmware', 'corex-rmk-firmware/build/target', '/cargo', '/rustup']
flags += ['--remap-path-prefix=' + source + '=' + target for source, target in zip(sys.argv[1:], destinations)]
print('\x1f'.join(flags))
PY
)
export CARGO_ENCODED_RUSTFLAGS
mkdir -p "$COREX_ROOT/build/firmware"
for COREX_HALF in right left; do
    if [ "$COREX_SIDE" != both ] && [ "$COREX_SIDE" != "$COREX_HALF" ]; then continue; fi
    cd "$COREX_ROOT/source/corex-rmk-pair/$COREX_HALF"
    if [ "$COREX_BOARD" != prototype ]; then
        cargo build --release --locked --features "$COREX_BOARD"
        COREX_NAME="CoreX-$COREX_MODEL-v$COREX_FIRMWARE_VERSION-$COREX_SOFTDEVICE"
    else
        cargo build --release --locked
        COREX_NAME="Cornix-Stock-Left-v$COREX_FIRMWARE_VERSION"
        COREX_UF2_TARGET="$COREX_HALF"
    fi
    COREX_ELF="$CARGO_TARGET_DIR/thumbv7em-none-eabihf/release/corex-pair-$COREX_HALF"
    "$COREX_OBJCOPY" -O binary "$COREX_ELF" "$COREX_ROOT/build/firmware/$COREX_NAME.bin"
    python3 "$COREX_ROOT/tools/make_uf2.py" "$COREX_UF2_TARGET" "$COREX_ROOT/build/firmware/$COREX_NAME.bin" "$COREX_ROOT/build/firmware/$COREX_NAME.uf2" --elf "$COREX_ELF"
    cp "$COREX_ELF" "$COREX_ROOT/build/firmware/$COREX_NAME.elf"
    if [ "$COREX_BOARD" != prototype ]; then
        "$COREX_OBJCOPY" -O ihex "$COREX_ELF" "$COREX_ROOT/build/firmware/$COREX_NAME.hex"
    fi
done
printf '\nBuild artifacts: %s/build/firmware\n' "$COREX_ROOT"
