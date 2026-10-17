#!/usr/bin/env python3
"""Copy an IBSP59 map with "<key>" "<value>" added to its worldspawn.

The grown entity lump (29) is appended at the end of the file and the lump
directory repointed, the same bytes `vcod_common::testing::bsp_with_worldspawn_key`
builds. Zip the result under the map's own path into a `zzz_*.pk3` in a
retail homepath's `main/` (`uo/` for UO) to run a stock map with a key no
stock worldspawn carries, such as `message` (configstring 4).

    tools/bsp_worldspawn_key.py in.bsp out.bsp message "vcod worldspawn message"
"""
import re
import struct
import sys

src, dst, key, value = sys.argv[1:5]
b = bytearray(open(src, "rb").read())
assert b[:4] == b"IBSP", "not an IBSP map"
d = 8 + 29 * 8
length, off = struct.unpack_from("<II", b, d)
ents = bytes(b[off:off + length]).rstrip(b"\0")
m = re.search(rb'"classname" "worldspawn"', ents)
assert m, "no worldspawn"
pair = b'\n"' + key.encode() + b'" "' + value.encode() + b'"'
ents = ents[:m.end()] + pair + ents[m.end():] + b"\0"
while len(b) % 4:
    b.append(0)
start = len(b)
b += ents
struct.pack_into("<II", b, d, len(ents), start)
open(dst, "wb").write(b)
