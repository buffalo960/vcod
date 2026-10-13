#!/usr/bin/env python3
"""Dump `bg_itemlist`, the classname table `BG_FindItem` searches for
`precacheItem`/`G_RegisterItem`, out of a game module: CoD 1.1d or 1.5b
`game.mp.i386.so`, or UO 1.51b `game.mp.uo.i386.so`.

    python3 tools/re/dump_itemlist.py game.mp.i386.so

Both `bg_itemlist` (stride 0x30, classname at +0x0, matching `BG_FindItem`'s
strcmp target) and `bg_numItems` (the row count) carry symbols in all three
modules. 1.1d: `bg_itemlist` `.data` 0x7b9d8, `bg_numItems` 70 at `.rodata`
0x70804, `BG_FindItem` 0x2e214, `G_RegisterItem` 0x4e504.

Unlike the function-pointer tables in `dump_builtins.py`, these are pointers
into the object's own `.rodata`, not calls to an exported symbol: the linker
emits an `R_386_RELATIVE` relocation whose stored placeholder already holds
the correct link-time address for a base-0 load, so the raw dword read from
the file *is* the string's vaddr directly (`gameso.Elf.ptr` reads either
kind). That is a second, distinct trap from the null-pointer one
`dump_builtins.py` documents for exported function pointers.

1.1: only indices 65-69 carry a real compiled-in classname: `item_ammo_stiel-
handgranate_open/closed` (65, 66) and `item_health_small/_health/_large`
(67-69), matching `docs/research/cod11-sound-system.md`'s independent find
from the cgame DLL's copy of this table. Indices 1-64 hold placeholder
classnames `emptyitem_"wNN"` — real weapon classnames like `mp40_mp` are
absent from the binary entirely (grep it) and only reach these slots at
runtime, from the mounted paks' weapon files, in an order this static dump
cannot recover. See docs/research/cod11-gsc-object-model.md for the
consequence for `Items::register`.
"""
import sys

from gameso import Elf

STRIDE = 0x30


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    e = Elf(sys.argv[1])
    itemlist = e.sym("bg_itemlist")
    num_items = e.word(e.sym("bg_numItems"))
    print(f"# bg_itemlist: {itemlist:#x} stride {STRIDE:#x}, bg_numItems={num_items}")
    for i in range(num_items):
        row = itemlist + i * STRIDE
        classname = e.cstr(e.ptr(row)) or ""
        pickup_sound = e.cstr(e.ptr(row + 4)) or ""
        print(f"  [{i:3}] {classname:<40} pickup_sound={pickup_sound!r}")


if __name__ == "__main__":
    main()
