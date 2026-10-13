#!/usr/bin/env python3
"""Dump the script builtin tables and the spawn classname table of a game
module: CoD 1.1d or 1.5b `game.mp.i386.so`, or UO 1.51b `game.mp.uo.i386.so`.

    python3 tools/re/dump_builtins.py game.mp.i386.so [table|all]

Builtins are not one table. `Scr_GetFunction` walks `functions` alone;
`Scr_GetMethod` tries the per-class tables and the generic entity methods in
the order printed on the first line, and the first table that knows a name
wins, so a name in an earlier table shadows the same name in a later one.
1.1 (0x5f724): Player, ScriptEnt, HudElem, then the generic table. 1.5
(0x62665): Player, ScriptEnt, generic, HudElem. UO (0x6e75f): Player,
ScriptEnt, ScriptVehicle, HudElem, generic. Each walk is a linear strcmp over
a fixed count, which is why the counts below are the loop bounds and not a
null terminator.

`functions` records are 12 bytes {char *name; void (*fn)(); int flag} and the
method records are 8 bytes {char *name; void (*fn)()}; a fixed 8-byte stride
over the whole region therefore garbles `functions`. `flag` is returned to
Scr_GetFunction's caller as a second out-parameter.

`spawns` is the classname table `G_CallSpawn` searches: a classname in it
becomes an engine entity built by its SP_ function, one not in it stays a
bare script-visible entity. Its records are 8 bytes and end at a null name.
A row marked `frees` has an SP_ function that calls `G_FreeEntity`, so the
block leaves no live entity and consumes no entity number; see
docs/research/cod11-gsc-object-model.md section 13.

Nothing is hardcoded per module: `functions` and `spawns` carry symbols, the
method tables are read off their lookup function's first table operand and
loop bound (`gameso.Elf.data_refs`, `loop_bound`), and the generic entity
table's lookup is the unnamed function `Scr_GetMethod` calls.
1.1d md5 de8947beb6f86fbfb46f5adfaab3d3ed, 1.5b 343f99cd67b79ac74aeaa5261f63c011,
UO 1.51b a4ffc2bfc3ae7af0b2374a23db3fa8ee.
"""
import sys

from gameso import Elf

# kind -> (lookup function, stride); None for the generic entity lookup,
# which has no symbol.
LOOKUPS = {
    "functions": ("Scr_GetFunction", 12),
    "player_methods": ("Player_GetMethod", 8),
    "scriptent_methods": ("ScriptEnt_GetMethod", 8),
    "vehicle_methods": ("ScriptVehicle_GetMethod", 8),
    "hudelem_methods": ("HudElem_GetMethod", 8),
    "entity_methods": (None, 8),
    "spawns": ("G_CallSpawn", 8),
}


def where(e, va):
    """`name+0xoff` off the nearest exported function at or below `va`."""
    best = max(((v, n) for n, (v, _) in e.func.items() if v <= va), default=None)
    if best is None:
        return f"{va:#x}"
    return best[1] if best[0] == va else f"{best[1]}+{va - best[0]:#x}"


def method_order(e):
    """Scr_GetMethod's calls in order: (label, lookup address)."""
    out = []
    for _, callee in e.call_targets("Scr_GetMethod"):
        if callee.startswith("0x"):
            out.append(("entity", int(callee, 16)))
        elif callee.endswith("_GetMethod"):
            out.append((callee[:-len("_GetMethod")], e.sym(callee)))
    return out


def tables(e):
    """kind -> (vaddr, stride, count or None, origin) for this module."""
    out = {}
    for kind, (fn, stride) in LOOKUPS.items():
        if fn is None:
            lookup = next(a for label, a in method_order(e) if label == "entity")
        elif fn in e.func:
            lookup = e.sym(fn)
        else:
            continue
        if kind == "functions":
            addr = e.sym("functions")
        elif kind == "spawns":
            addr = e.sym("spawns")
        else:
            addr = e.data_refs(lookup)[0]
        count = None if kind == "spawns" else e.loop_bound(lookup)
        out[kind] = (addr, stride, count, f"{where(e, lookup)} {lookup:#x}")
    return out


def dump(e, kind, table):
    addr, stride, count, origin = table
    print(f"# {kind}: {addr:#x} stride {stride}, from {origin}")
    i = 0
    while count is None or i < count:
        v = addr + i * stride
        name = e.cstr(e.word(v))
        if name is None:
            break
        fn = e.rels.get(v + 4) or f"{e.ptr(v + 4):#x}"
        extra = f"  flag={e.word(v + 8)}" if stride == 12 else ""
        # An SP_ function that is nothing but G_FreeEntity(self) leaves the
        # block with no live entity and so consumes no entity number.
        if kind == "spawns" and e.calls(fn, "G_FreeEntity"):
            extra += "  frees"
        print(f"  [{i:3}] {name:<28} {fn}{extra}")
        i += 1
    print(f"# {i} entries\n")


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    e = Elf(sys.argv[1])
    which = sys.argv[2] if len(sys.argv) > 2 else "all"
    found = tables(e)
    if which == "all":
        order = ", ".join(f"{label} {a:#x}" for label, a in method_order(e))
        print(f"# Scr_GetMethod {e.sym('Scr_GetMethod'):#x} tries: {order}\n")
    elif which not in found:
        sys.exit(f"no {which} table in this module ({', '.join(found)})")
    for kind in (found if which == "all" else [which]):
        dump(e, kind, found[kind])


if __name__ == "__main__":
    main()
