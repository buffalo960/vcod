#!/usr/bin/env python3
"""Dump the script field tables out of a game module: CoD 1.1d or 1.5b
`game.mp.i386.so`, or UO 1.51b `game.mp.uo.i386.so`.

`GScr_AddFieldsFor*` walk static arrays of field records and register each one
with the script engine, which is what decides whether a field name on a script
object is engine-backed or a plain script-defined key. This prints those arrays.

    python3 tools/re/dump_script_fields.py game.mp.i386.so [entity|client|hudelem|vehiclenode|all]

Record layouts (all end at a null name pointer):

    entity       16 bytes  {char *name; int offset; int type; void (*set)();}
    client       20 bytes  {char *name; int offset; int type; void (*set)(); void (*get)();}
    hudelem      20 bytes  same as client
    vehiclenode  12 bytes  {char *name; int offset; int type;}  (UO only)

A hook is a pointer stored in `.data`, so it reads as 0 in the file whenever a
relocation supplies it; this reads the relocations through gameso.Elf and
prints the symbol name in that case. Each table is the first table operand of
its `GScr_AddFieldsFor*` function, so no address is hardcoded.

`type` selects the storage conversion in Scr_GetGenericField/Scr_SetGenericField.
1.1 (0x6248c / 0x6257c; jump table 0x7968c) and 1.5 (0x65f50) share one enum,
TYPES below. UO renumbered it (Scr_GetGenericField 0x72131, UO_TYPES); 1, 2
and 8 have no case there. Only the vehicle-node key parser (0x95e32) reads 1
and 2; nothing in the module reads 8 (coduo-diff.md section 13). A record whose type the registering function
rejects is skipped and never becomes a script field (ACCEPTED, read off each
`GScr_AddFieldsFor*`'s compare chain or switch). Client field ids are tagged
`| 0xC000`, which is how Scr_GetEntityField (1.1 0x62824) routes an id to
`ent->client` instead of the entity table; the others are plain indices.
"""
import struct
import sys

from pathlib import Path

# The sibling helper, found under `python3 -I` too.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from gameso import Elf  # noqa: E402

# kind -> (registering function, stride, id tag).
TABLES = {
    "entity": ("GScr_AddFieldsForEntity", 16, 0x0000),
    "client": ("GScr_AddFieldsForClient", 20, 0xC000),
    "hudelem": ("GScr_AddFieldsForHudElems", 20, 0x0000),
    "vehiclenode": ("GScr_AddFieldsForVehicleNode", 12, 0x0000),
}

# Scr_GetGenericField's switch, in index order. The parenthesised note is what
# the script sees; "undefined" means a zero stored value reads as undefined.
TYPES = {
    0: "int",
    1: "float",
    2: "string (char[] in place)",
    3: "string (u16 script-string, 0 -> undefined)",
    4: "vector (3 floats)",
    5: "entity (gentity_t*, null -> undefined)",
    6: "vector (0, float, 0)",
    7: "object (u16 handle, 0 -> undefined)",
    8: "string (u8 model index via G_ModelName)",
}

UO_TYPES = {
    0: TYPES[0],
    3: TYPES[1],
    4: TYPES[2],
    5: TYPES[3],
    6: TYPES[4],
    7: TYPES[5],
    9: TYPES[6],
    10: TYPES[7],
    11: TYPES[8],
    # Spawn keys only: the vehicle-node parser's atoi into a short or a byte.
    1: "short (spawn key only, no script getter)",
    2: "byte (spawn key only, no script getter)",
    8: "unused (no case in the module)",
    12: "unused (no case in the module)",
}

# 1.1 and 1.5 register the same sets (1.5 0x65bbf, 0x3bd88, 0x4965c).
ACCEPTED = {
    "entity": lambda t: t <= 5 or t in (7, 8),
    "client": lambda t: t <= 5 or t == 7,
    "hudelem": lambda t: t <= 5 or t in (7, 8),
}

# UO: entity 0x71d79 and hudelem 0x52659 switch, client 0x44082 compares,
# vehiclenode 0x983d4 switch.
UO_ACCEPTED = {
    "entity": lambda t: t in (0, 3, 4, 5, 6, 7, 10, 11),
    "client": lambda t: t in (0, 3, 4, 5, 6, 7, 10),
    "hudelem": lambda t: t in (0, 3, 4, 5, 6, 7, 10, 11),
    "vehiclenode": lambda t: t in (0, 1, 2, 3, 5, 6, 7, 9),
}


def dump(e, kind):
    fn, stride, tag = TABLES[kind]
    uo = e.flavour() == "uo"
    types, accepted = (UO_TYPES, UO_ACCEPTED) if uo else (TYPES, ACCEPTED)
    origin = f"{fn} {e.sym(fn):#x}"
    table = e.data_refs(fn)[0]
    base = e.vf(table)
    if base is None:
        sys.exit(f"table {table:#x} is not in a loaded section")
    print(f"# {kind}: {table:#x} stride {stride}, from {origin}")
    i = 0
    while True:
        v = table + i * stride
        rec = e.d[base + i * stride: base + i * stride + stride]
        name_ptr, offset, typ = struct.unpack("<Iii", rec[:12])
        if name_ptr == 0:
            break
        hooks = []
        for j in range(12, stride, 4):
            word, = struct.unpack("<I", rec[j:j + 4])
            sym = e.rels.get(v + j)
            if sym or word:
                hooks.append(sym or f"{word:#x}")
        kept = accepted[kind](typ)
        print(f"  [{i:3}] id={tag | i:#06x} {e.cstr(name_ptr)!s:<22}"
              f" off={offset:<5} type={typ} {types.get(typ, '?'):<40}"
              f"{'' if kept else ' SKIPPED'}"
              f"{(' hooks ' + ' '.join(hooks)) if hooks else ''}")
        i += 1
    print(f"# {i} records\n")


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    e = Elf(sys.argv[1])
    which = sys.argv[2] if len(sys.argv) > 2 else "all"
    kinds = [k for k in TABLES if TABLES[k][0] in e.func]
    if which != "all" and which not in kinds:
        sys.exit(f"no {which} table in this module ({', '.join(kinds)})")
    for kind in (kinds if which == "all" else [which]):
        dump(e, kind)


if __name__ == "__main__":
    main()
