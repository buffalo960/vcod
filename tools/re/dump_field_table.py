#!/usr/bin/env python3
"""Dump a netField table out of a dedicated-server ELF (`cod_lnxded` 1.1d or
1.5b, `coduo_lnxded` 1.51b).

Each netField is 12 bytes: {char *name; int offset; int bits}. Given a table's
virtual address, print its entries with the string each name pointer resolves to.

Written for the playerState delta: its array blocks delta against a 34-entry
HUD-element table at 0x80de384 that has no symbol, so the field widths had to be
read straight out of .data.

    python3 tools/re/dump_field_table.py cod_lnxded 0x80de384 34

Known tables (cod_lnxded md5 49717db56f6da717545838ce88d4865e):
    0x080d1760  59   entityStateFields   (snapshot entities)
    0x080d229c  103  playerStateFields
    0x080d2058  22   clientState fields
    0x080de384  34   HUD-element fields  (playerState array blocks 4 and 5)

1.5b (docs/research/cod15-protocol-diff.md section 4): entityState 0x080db860
(59), clientState 0x080dbfe0 (22), playerState 0x080dc560 (103), objective
and HUD-element fields 0x080e9a80 (34). UO 1.51b (docs/research/coduo-diff.md
section 4): entityState 0x080e11a0 (60), playerState 0x080e2260 (114),
HUD-element fields 0x080f03c0 (30), among others. The executables are
non-PIC, so the name pointers are plain vaddrs in all three.
"""
import struct
import sys


def sections(data):
    e_shoff = struct.unpack("<I", data[0x20:0x24])[0]
    e_shentsize = struct.unpack("<H", data[0x2E:0x30])[0]
    e_shnum = struct.unpack("<H", data[0x30:0x32])[0]
    out = []
    for i in range(e_shnum):
        off = e_shoff + i * e_shentsize
        _, _, _, addr, offset, size = struct.unpack("<IIIIII", data[off:off + 24])
        out.append((addr, offset, size))
    return out


def vaddr_to_file(secs, v):
    for addr, offset, size in secs:
        if addr and addr <= v < addr + size:
            return offset + (v - addr)
    return None


def cstr(data, secs, v):
    f = vaddr_to_file(secs, v)
    if f is None:
        return None
    end = data.index(b"\0", f)
    return data[f:end].decode("latin1")


def main():
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    binary, table, count = sys.argv[1], int(sys.argv[2], 0), int(sys.argv[3], 0)
    data = open(binary, "rb").read()
    secs = sections(data)
    base = vaddr_to_file(secs, table)
    if base is None:
        sys.exit(f"vaddr {table:#x} is not in a loaded section")
    print(f"table {table:#x} ({count} entries) at file {base:#x}")
    for i in range(count):
        name_ptr, offset, bits = struct.unpack("<Iii", data[base + i * 12: base + i * 12 + 12])
        print(f"  [{i:3}] {cstr(data, secs, name_ptr)!s:<24} offset={offset:<5} bits={bits}")


if __name__ == "__main__":
    main()
