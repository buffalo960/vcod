#!/usr/bin/env python3
"""Dump a game module's cvar table: CoD 1.1d or 1.5b `game.mp.i386.so`, or
UO 1.51b `game.mp.uo.i386.so`.

`G_RegisterCvars` walks a static array of `{vmCvar_t *vmCvar; char *name;
char *defaultString; int flags; int trackChange; int teamShader;}` records and
registers each one, which is where every `g_*`/`bg_*` default a script can read
comes from. This prints that array.

    python3 tools/re/dump_cvars.py game.mp.i386.so [name-substring]

The table is the `gameCvarTable` symbol, 24-byte records (the stride
`G_RegisterCvars` steps in all three), ending at a null name pointer. 1.1d:
`.data` 0x7de28, 71 rows. Flag 0x800 marks the 21 rows that reach the 140/204 configstring
mirror; those are `crates/server/src/cvars.rs`'s `ENGINE_MIRRORED`. A row
without it is invisible in a configstring capture and can only be read here,
which is what made `g_useGear` cost a measurement round -- see
docs/research/cod11-gsc-object-model.md section 18.

Same relocation trap as `dump_itemlist.py`: these are `R_386_RELATIVE`
pointers into the module's own `.rodata`, so the raw dword read from the file
is the string's vaddr directly (`gameso.Elf.ptr`).
"""
import sys

from gameso import Elf

STRIDE = 24


def main():
    if not 2 <= len(sys.argv) <= 3:
        sys.exit(__doc__)
    e = Elf(sys.argv[1])
    want = sys.argv[2].lower() if len(sys.argv) == 3 else None
    cvars = e.sym("gameCvarTable")
    print(f"# game cvar table: {cvars:#x} stride {STRIDE:#x}")
    va = cvars
    i = 0
    while True:
        name = e.cstr(e.ptr(va + 4))
        if not name:
            break
        default = e.cstr(e.ptr(va + 8)) or ""
        flags = e.word(va + 12)
        if want is None or want in name.lower():
            mirror = " mirrored" if flags & 0x800 else ""
            print(f"  [{i:3}] {name:<32} = {default!r:<36} flags={flags:#x}{mirror}")
        va += STRIDE
        i += 1
    print(f"# {i} records")


if __name__ == "__main__":
    main()
