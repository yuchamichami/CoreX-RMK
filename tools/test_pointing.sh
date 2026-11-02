#!/bin/sh
set -eu
COREX_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
# Match the combo capacity of the shipping right-side configuration.
export KEYBOARD_TOML_PATH="$COREX_ROOT/source/corex-rmk-pair/right/keyboard-raytac.toml"
COREX_RUSTC=$(rustup which --toolchain 1.95.0 rustc)
export PATH="$(dirname "$COREX_RUSTC"):$PATH"
COREX_HOST=$(rustup run 1.95.0 rustc -vV | sed -n 's/^host: //p')
mkdir -p "$COREX_ROOT/build/tests/pointing"
rustup run 1.95.0 cargo test --locked \
    --manifest-path "$COREX_ROOT/tools/pointing-host-tests/Cargo.toml" \
    --target "$COREX_HOST" --target-dir "$COREX_ROOT/build/tests/ble" \
    --lib --no-run --message-format=json > "$COREX_ROOT/build/tests/pointing/artifacts.jsonl"
python3 - "$COREX_ROOT/build/tests/pointing/artifacts.jsonl" <<'PY'
import json, os, subprocess, sys
items = [json.loads(line) for line in open(sys.argv[1]) if line.strip()]
exes = [a['executable'] for a in items if a.get('reason') == 'compiler-artifact' and a.get('executable') and a.get('profile',{}).get('test') and a['target']['name'] == 'corex_pointing_host_tests']
assert len(exes) == 1
names = [line.removesuffix(': test') for line in subprocess.check_output([exes[0], '--list', '--format', 'terse'], text=True).splitlines() if line.endswith(': test')]
assert names
for name in names:
    subprocess.run([exes[0], '--exact', name], check=True, env={**os.environ, 'RMK_TEST_PROCESS_ISOLATED':'1'})
print(f'PASS {len(names)} pointing tests, each in a fresh process')
PY
