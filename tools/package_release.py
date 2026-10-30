#!/usr/bin/env python3
"""Verify all board builds before publishing application-only UF2 files."""
import hashlib
import json
import lzma
import re
import shutil
import struct
from pathlib import Path

from verify_release import require, verify, verify_rebuilt_schema
from configuration_contract import configuration_contract
from make_uf2 import convert

ROOT = Path(__file__).resolve().parents[1]


def main():
    version = (ROOT / 'source/corex-rmk-pair/VERSION').read_text().strip()
    manifest = dict(
        distribution='CoreX / Cornix TB', release='v' + version,
        rust_toolchain='1.95.0', target='thumbv7em-none-eabihf',
        rmk_base_commit='8a6889854fb996be592c55075b385234133e1772',
        rmk_features='_ble,_nrf_ble,adafruit_bl,async_matrix,host,log,nrf52840_ble,split,storage,usb_log,vial',
        storage_schema_hash='0xA805CEFB',
        device_names={'right_bluetooth': 'Cornix TB', 'right_usb_product': 'Cornix TB'},
        validation_scope='Application builds, UF2 bounds/vectors/checksums, compiled Vial definition and storage schema verified. This release has not been flashed or tested over Bluetooth on hardware.',
        images=[])
    for half in ['right', 'left', 'stock-left']:
        target = ROOT / 'build/target' / half
        verify_rebuilt_schema(ROOT, target, manifest, required=True)
        metadata = max((p for p in target.glob('thumbv7em-none-eabihf/release/build/rmk-*/output')
                        if re.fullmatch(r'rmk-[0-9a-f]+', p.parent.name)),
                       key=lambda p: p.stat().st_mtime).read_text()
        require(f'cargo:rustc-env=COREX_FIRMWARE_VERSION={version}\n' in metadata, 'Stale compiled version')
        expected_board = {'right': 'raytac-r-a14', 'left': 'raytac-l-rev1', 'stock-left': 'prototype'}[half]
        require(f'cargo:rustc-env=COREX_BOARD={expected_board}\n' in metadata, 'Stale compiled board')
    generated = max((ROOT / 'build/target/right').glob(
        'thumbv7em-none-eabihf/release/build/corex-pair-right-*/out/config_generated.rs'),
        key=lambda p: p.stat().st_mtime)
    array = re.search(r'VIAL_KEYBOARD_DEF:.*?= &\[(.*?)\];', generated.read_text()).group(1)
    definition = bytes(int(n) for n in re.findall(r'(\d+)u8', array))
    require(json.loads(lzma.decompress(definition)) == json.loads(
        (ROOT / 'source/corex-rmk-pair/right/vial-raytac.json').read_text()), 'Stale Vial definition')
    id_array = re.search(r'VIAL_KEYBOARD_ID:.*?= &\[(.*?)\];', generated.read_text()).group(1)
    uid = bytes(int(n) for n in re.findall(r'(\d+)u8', id_array))
    manifest['configuration_profile'] = configuration_contract(definition, uid)
    variants = [(f'CoreX-{label}-v{version}-{sd}', f'raytac-{half}-{base}')
                for half, label in [('right', 'Right'), ('left', 'Left')]
                for sd, base in [('s140v6', '26000'), ('s140v7', '27000')]]
    variants.append((f'Cornix-Stock-Left-v{version}', 'left'))
    for stem, side in variants:
        path = ROOT / 'build/firmware' / (stem + '.uf2')
        binary = path.with_suffix('.bin').read_bytes()
        require(convert(side, binary, path.with_suffix('.elf').read_bytes())[0] == path.read_bytes(),
                'UF2 does not match its linked ELF')
        image = dict(side=side, filename=path.name, application_version=version,
                     binary_bytes=len(binary), binary_sha256=hashlib.sha256(binary).hexdigest(),
                     uf2_sha256=hashlib.sha256(path.read_bytes()).hexdigest())
        require(verify(image, path) == binary, 'UF2 and binary differ')
        if side.startswith('raytac-right'):
            require(definition in binary, 'Compiled definition missing from firmware')
            require(b'Cornix TB' in binary, 'New host name missing from firmware')
            require(b'CoreX Raytac' not in binary, 'Old host name remains in firmware')
        start = struct.unpack_from('<I', path.read_bytes(), 12)[0]
        image.update(flash_start=hex(start), flash_end_exclusive=hex(start + len(path.read_bytes()) // 512 * 256))
        manifest['images'].append(image)
    # Copy only after every variant passed. Reset images and raw flash dumps are never packaged.
    out = ROOT / 'firmware'
    out.mkdir(exist_ok=True)
    for image in manifest['images']:
        shutil.copy2(ROOT / 'build/firmware' / image['filename'], out / image['filename'])
    (out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    (out / 'SHA256SUMS').write_text(''.join(f"{i['uf2_sha256']}  {i['filename']}\n" for i in manifest['images']))
    print('PASS packaged five application-only variants; configuration identity and storage schema preserved')


if __name__ == '__main__':
    main()
