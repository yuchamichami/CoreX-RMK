#!/usr/bin/env python3
"""Run physical thumb-key scenarios against RMK with one process per case."""
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
host = next(line.split(": ", 1)[1] for line in subprocess.check_output(
    ["rustup", "run", "1.95.0", "rustc", "-vV"], text=True).splitlines() if line.startswith("host: "))
result = subprocess.check_output([
    "rustup", "run", "1.95.0", "cargo", "test", "--locked",
    "--manifest-path", str(ROOT / "source/corex-rmk-upstream/rmk/Cargo.toml"),
    "--no-default-features", "--features", "std,log,_ble,split,vial,host_lock",
    "--target", host, "--target-dir", str(ROOT / "build/tests/ble"),
    "--test", "integration", "--no-run", "--message-format=json"], text=True)
artifacts = [json.loads(line) for line in result.splitlines() if line.startswith("{")]
executables = [a["executable"] for a in artifacts if a.get("reason") == "compiler-artifact"
               and a.get("executable") and a["target"]["name"] == "integration"]
assert len(executables) == 1, executables
executable = executables[0]
names = [line.removesuffix(": test") for line in subprocess.check_output(
    [executable, "--list", "--format", "terse"], text=True).splitlines()
    if line.startswith(("corex_language_thumbs::", "corex_right_inspection::")) and line.endswith(": test")]
assert len(names) == 14, names
for name in names:
    subprocess.run([executable, "--exact", name], check=True,
                   env={**os.environ, "RMK_TEST_PROCESS_ISOLATED": "1"})
print("PASS 14 CoreX language/inspection input/output scenarios in separate processes")
