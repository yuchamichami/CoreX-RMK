#!/bin/sh
# Run BLE and log privacy tests on the host, without an ARM toolchain or hardware.
set -eu
COREX_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
COREX_RUSTC=$(rustup which --toolchain 1.95.0 rustc)
export PATH="$(dirname "$COREX_RUSTC"):$PATH"
COREX_HOST=$("$COREX_RUSTC" -vV | sed -n 's/^host: //p')
if [ -z "$COREX_HOST" ]; then
    echo "Unable to determine the Rust host target" >&2
    exit 1
fi

# Build once, then isolate every test in a fresh process. RMK's mock clock,
# channels and sleep state are process-global, even with --test-threads=1.
mkdir -p "$COREX_ROOT/build/tests/ble"
COREX_METADATA="$COREX_ROOT/build/tests/ble/test-artifacts.jsonl"
rustup run 1.95.0 cargo test --locked \
    --manifest-path "$COREX_ROOT/source/corex-rmk-upstream/rmk/Cargo.toml" \
    --no-default-features --features std,log,_ble,split,vial,host_lock \
    --target "$COREX_HOST" --target-dir "$COREX_ROOT/build/tests/ble" \
    --lib --no-run --message-format=json > "$COREX_METADATA"
python3 - "$COREX_METADATA" <<'PYTEST'
import json, os, subprocess, sys
artifacts = [json.loads(line) for line in open(sys.argv[1]) if line.strip()]
executables = [a['executable'] for a in artifacts
               if a.get('reason') == 'compiler-artifact' and a.get('executable')
               and a.get('profile', {}).get('test') and a['target']['name'] == 'rmk']
assert len(executables) == 1, executables
exe = executables[0]
names = [line.removesuffix(': test') for line in
         subprocess.check_output([exe, '--list', '--format', 'terse'], text=True).splitlines()
         if line.endswith(': test')]
prefixes = ('ble::battery_service::', 'ble::report_writer::', 'ble::ble_server::tests::', 'ble::tests::', 'usb::', 'ble::sleep::tests::',
            'split::', 'keyboard::tests::',
            'keyboard::auto_mouse_layer::', 'keyboard::test::', 'host::via::', 'host::lock::', 'state::tests::',
            'input_device::pointing::', 'keymap::', 'storage::factory_reset_tests::')
selected = [name for name in names if name.startswith(prefixes)]
assert selected, 'No host regression tests selected'
for name in selected:
    subprocess.run([exe, '--exact', name], check=True,
                   env={**os.environ, 'RMK_TEST_PROCESS_ISOLATED': '1'})
print(f'PASS {len(selected)} RMK host tests, each in a fresh process')
PYTEST
