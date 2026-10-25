#!/usr/bin/env python3
"""Generate application-only UF2 after checking the linked ARM ELF and binary."""
import argparse
import hashlib
import json
from pathlib import Path
import struct

TARGETS = {
    "left": (0x1000, 0xA0000),
    "raytac-right-26000": (0x26000, 0xB0000),
    "raytac-right-27000": (0x27000, 0xB0000),
    "raytac-left-26000": (0x26000, 0xA0000),
    "raytac-left-27000": (0x27000, 0xA0000),
}

def linked_binary(elf, base, limit):
    """Read ELF32 PT_LOAD physical addresses, including the RAM data's flash LMA.

    A raw BIN contains no link address. Its reset vector alone cannot distinguish
    a v7 image incorrectly moved down to the v6 base, so require the ELF that
    produced it and compare every flash byte before assigning UF2 addresses.
    """
    if len(elf) < 52 or elf[:7] != b"\x7fELF\x01\x01\x01":
        raise ValueError("Expected a little-endian ELF32 executable")
    header = struct.unpack_from("<16sHHIIIIIHHHHHH", elf)
    _, kind, machine, version, entry, phoff, _, _, ehsize, phsize, phcount, *_ = header
    if (kind, machine, version, ehsize, phsize) != (2, 40, 1, 52, 32) or not phcount:
        raise ValueError("Expected an ARM executable with ELF32 load headers")
    if phoff < ehsize or phoff + phcount * phsize > len(elf):
        raise ValueError("Truncated ELF load headers")
    segments = []
    executable = []
    for index in range(phcount):
        kind, offset, address, physical, size, memory_size, flags, _ = struct.unpack_from(
            "<8I", elf, phoff + index * phsize)
        if kind != 1:
            continue
        if size > memory_size or offset + size > len(elf):
            raise ValueError("Invalid ELF load segment")
        if not size:  # Uninitialized RAM has no bytes in the flash image.
            continue
        if physical < base or physical + size > limit:
            raise ValueError("ELF load address is outside the selected application's flash")
        segments.append((physical, elf[offset:offset + size]))
        if flags & 1:
            executable.append((address, address + size))
    segments.sort()
    if not segments or segments[0][0] != base:
        raise ValueError("ELF link base does not match the selected firmware target")
    if not entry & 1 or not any(start <= (entry & ~1) < end for start, end in executable):
        raise ValueError("ELF entry point is not executable Thumb code")
    image = bytearray()
    for address, payload in segments:
        offset = address - base
        if offset < len(image):
            raise ValueError("Overlapping ELF load segments")
        image.extend(b"\0" * (offset - len(image)))
        image.extend(payload)
    if len(image) < 8 or struct.unpack_from("<I", image, 4)[0] != entry:
        raise ValueError("ELF entry point and application reset vector differ")
    return bytes(image)


def convert(side, data, elf):
    if side not in TARGETS:
        raise ValueError("Unknown firmware target")
    base, limit = TARGETS[side]
    if linked_binary(elf, base, limit) != data:
        raise ValueError("Binary does not match the linked ELF flash image")
    if len(data) < 8:
        raise ValueError("The application does not contain a vector table")
    stack, reset = struct.unpack_from("<II", data)
    if not (0x20000008 <= stack <= 0x20040000 and stack % 8 == 0):
        raise ValueError("Invalid nRF52840 initial stack pointer")
    if not (reset & 1 and base <= (reset & ~1) < base + len(data)):
        raise ValueError("Reset handler does not belong to the selected side's application")
    count = (len(data) + 255) // 256
    if base + count * 256 > limit:
        raise ValueError("Application would overlap persistent settings")
    blocks = []
    for index in range(count):
        payload = data[index * 256:(index + 1) * 256].ljust(256, b"\0")
        header = struct.pack("<8I", 0x0A324655, 0x9E5D5157, 0x2000,
                             base + index * 256, 256, index, count, 0xADA52840)
        blocks.append(header + payload + b"\0" * 220 + struct.pack("<I", 0x0AB16F30))
    return b"".join(blocks), base, base + count * 256


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("side", choices=list(TARGETS))
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--elf", type=Path, required=True,
                        help="matching linked ARM ELF; required to verify the load address")
    args = parser.parse_args()
    try:
        data = args.binary.read_bytes()
        uf2, base, end = convert(args.side, data, args.elf.read_bytes())
    except (ValueError, OSError) as error:
        parser.error(str(error))
    args.output.write_bytes(uf2)
    print(json.dumps({"side": args.side, "uf2": args.output.name,
                      "binary_bytes": len(data), "range": [hex(base), hex(end)],
                      "sha256": hashlib.sha256(uf2).hexdigest()}))


if __name__ == "__main__":
    main()
