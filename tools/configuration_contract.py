"""Public configuration identity, derived from firmware inputs rather than a UI."""
import hashlib
import json
import lzma
import re
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def configuration_contract(definition, uid):
    board = ROOT / 'source/corex-rmk-pair/right'
    base = tomllib.loads((board / 'keyboard.toml').read_text())
    production = tomllib.loads((board / 'keyboard-raytac.toml').read_text())
    positions = lambda config: [tuple(map(int, pair)) for pair in
        re.findall(r'\((\d+),(\d+)\)', config['layout']['map'])]
    old, new = positions(base), positions(production)
    if not len(old) == len(new) == len(set(old)) == len(set(new)) == 56:
        raise ValueError('Invalid configuration matrix mapping')
    if json.loads(lzma.decompress(definition)) != json.loads((board / 'vial-raytac.json').read_text()):
        raise ValueError('Compiled Vial definition differs from firmware source')
    if uid != b'CoreXRY1':
        raise ValueError('Unexpected compiled Vial identity')
    version = [int(n) for n in (ROOT / 'source/corex-rmk-pair/VERSION').read_text().strip().split('.')]
    if len(version) != 3 or any(n < 0 or n > 255 for n in version):
        raise ValueError('Firmware version cannot be encoded')
    return dict(uid=uid.hex(), firmware=version[0] << 16 | version[1] << 8 | version[2],
                pointingDevices=['trackball'], gestures=True, scrollDirection=True,
                comboSlots=production.get("rmk", {}).get("combo_max_num", 8),
                backupCompatibleFirmwares=[0x0B09],
                definitionSize=len(definition), definitionSha256=hashlib.sha256(definition).hexdigest(),
                matrixMap=[list(a + b) for a, b in zip(old, new) if a != b])
