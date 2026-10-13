"""ELF reader shared by the `dump_*.py` scripts, for the three game modules.

    1.1d  game.mp.i386.so     non-PIC: tables addressed by absolute operands
                              that `.rel.text` relocates, pointers in `.data`
                              relocated through `.rel.data`
    1.5b  game.mp.i386.so     PIC: tables addressed off %ebx (the GOT), either
    UO    game.mp.uo.i386.so  as GOTOFF (`lea -0x4a58(%ebx)`) or through a GOT
                              slot (`mov 0xb9c(%ebx),%ecx`); every relocation
                              in one `.rel.dyn`

A pointer stored in `.data` is 0 in the file when an `R_386_32` relocation
supplies it (a function pointer to an exported symbol), and is the target's
vaddr when an `R_386_RELATIVE` one does (a string in `.rodata`): `ptr` reads
either. Tables without a symbol are found from the code that walks them
(`data_refs`, `loop_bound`), so one code path serves all three modules.
"""
import re
import struct
import subprocess

R_386_32 = 1
R_386_RELATIVE = 8


class Elf:
    def __init__(self, path):
        self.path = path
        d = self.d = open(path, "rb").read()
        shoff, = struct.unpack("<I", d[0x20:0x24])
        entsize, = struct.unpack("<H", d[0x2E:0x30])
        num, = struct.unpack("<H", d[0x30:0x32])
        stridx, = struct.unpack("<H", d[0x32:0x34])
        self.secs = []
        for i in range(num):
            o = shoff + i * entsize
            nm, typ, _, addr, off, size, link = struct.unpack("<IIIIIII", d[o:o + 28])
            self.secs.append(dict(nm=nm, typ=typ, addr=addr, off=off, size=size, link=link))
        base = self.secs[stridx]["off"]
        for s in self.secs:
            o = base + s["nm"]
            s["n"] = d[o:d.index(b"\0", o)].decode()
        dyn = self.sec(".dynsym")
        dstr = self.sec(".dynstr")
        syms = []
        self.func = {}
        self.syms = {}
        for i in range(dyn["size"] // 16):
            o = dyn["off"] + i * 16
            nm, val, size = struct.unpack("<III", d[o:o + 12])
            name = d[dstr["off"] + nm:d.index(b"\0", dstr["off"] + nm)].decode()
            syms.append((name, val))
            if val:
                self.syms.setdefault(name, val)
            if val and size:
                self.func.setdefault(name, (val, size))
        # offset -> (type, symbol name, symbol value); every REL section, so
        # 1.1's split .rel.* and 1.5/UO's single .rel.dyn read alike.
        self.relocs = {}
        for s in self.secs:
            if s["typ"] != 9 or s["link"] != self.secs.index(dyn):
                continue
            for i in range(s["size"] // 8):
                off, info = struct.unpack("<II", d[s["off"] + i * 8:s["off"] + i * 8 + 8])
                name, val = syms[info >> 8]
                self.relocs[off] = (info & 0xFF, name, val)
        # offset -> symbol name ('' for R_386_RELATIVE), the old interface.
        self.rels = {o: r[1] for o, r in self.relocs.items()}
        self.got = self.syms.get("_GLOBAL_OFFSET_TABLE_")
        self._dis = {}

    def sec(self, n):
        return next((s for s in self.secs if s["n"] == n), None)

    def in_sec(self, v, *names):
        return any(s["addr"] <= v < s["addr"] + s["size"]
                   for s in (self.sec(n) for n in names) if s)

    def vf(self, v):
        for s in self.secs:
            if s["addr"] and s["addr"] <= v < s["addr"] + s["size"] and s["typ"] != 8:
                return s["off"] + (v - s["addr"])
        return None

    def cstr(self, v):
        f = self.vf(v) if v else None
        return None if f is None else self.d[f:self.d.index(b"\0", f)].decode("latin1")

    def word(self, v):
        f = self.vf(v)
        return struct.unpack("<I", self.d[f:f + 4])[0]

    def sym(self, name):
        v = self.syms.get(name)
        if v is None:
            raise SystemExit(f"{self.path}: no symbol {name}")
        return v

    def ptr(self, v):
        """The pointer the loader stores at `v`, as a link-time vaddr."""
        r = self.relocs.get(v)
        if r and r[0] != R_386_RELATIVE and r[1]:
            return (r[2] + self.word(v)) & 0xFFFFFFFF
        return self.word(v)

    def calls(self, name, callee):
        """Whether the exported function `name` relocates against `callee`.

        On 1.1 a call is an `R_386_PC32` against the callee; on 1.5 and UO it
        goes through the PLT, so the disassembly's `<callee@plt>` is read too.
        """
        span = self.func.get(name)
        if span is None:
            return False
        va, size = span
        if any(self.rels.get(o) == callee for o in range(va, va + size)):
            return True
        return any(t == callee for _, t in self.call_targets(name))

    # --- disassembly -------------------------------------------------------

    def disasm(self, where):
        """[(addr, mnemonic, operands, reloc)] for a function: a symbol name,
        or a vaddr (read up to its first `ret`)."""
        if where in self._dis:
            return self._dis[where]
        if isinstance(where, str):
            start, size = self.func[where]
            end = start + size
        else:
            start, end = where, where + 0x400
        out = subprocess.run(
            ["objdump", "-dR", "--no-show-raw-insn", f"--start-address={start:#x}",
             f"--stop-address={end:#x}", self.path],
            capture_output=True, text=True, check=True).stdout
        ins = []
        for line in out.splitlines():
            m = re.match(r"\s+([0-9a-f]+):\s+(R_386_\w+)\s+(\S+)", line)
            if m and ins:
                ins[-1][3] = (m.group(2), m.group(3).split("@")[0])
                continue
            m = re.match(r"\s+([0-9a-f]+):\t(\S+)\s*(.*)$", line)
            if m:
                ins.append([int(m.group(1), 16), m.group(2), m.group(3), None])
                if not isinstance(where, str) and m.group(2) == "ret":
                    break
        self._dis[where] = ins
        return ins

    def _pic(self, ins):
        # The PIC prologue: `call thunk; add $imm,%ebx` leaves the GOT in %ebx.
        return self.got is not None and any(
            a[1] == "call" and b[1] == "add" and b[2].startswith("$") and b[2].endswith(",%ebx")
            for a, b in zip(ins, ins[1:]))

    def data_refs(self, where, secs=(".data", ".rodata")):
        """Addresses in `secs` the function's operands name, in code order."""
        ins = self.disasm(where)
        pic = self._pic(ins)
        refs = []
        for _, mn, ops, rel in ins:
            v = None
            m = re.search(r"(-?0x[0-9a-f]+)\(%ebx[,)]", ops)
            if pic and m:
                v = self.got + int(m.group(1), 16)
                # `mov disp(%ebx),%reg` with disp in the GOT loads a pointer.
                if mn == "mov" and re.match(r"-?0x[0-9a-f]+\(%ebx\),%e", ops) \
                        and self.in_sec(v, ".got"):
                    v = self.ptr(v)
            elif not pic:
                if mn != "call" and not mn.startswith("j"):
                    for h in re.findall(r"0x[0-9a-f]+", ops):
                        v = int(h, 16)
                        if rel and rel[0] == "R_386_32":
                            v += self.syms.get(rel[1], 0)
                        if self.in_sec(v, *secs) and v not in refs:
                            refs.append(v)
                    v = None
            if v is not None and self.in_sec(v, *secs) and v not in refs:
                refs.append(v)
        return refs

    def loop_bound(self, where):
        """Entry count of a lookup loop: the `cmp $n,..` an unsigned `jbe`
        follows, i.e. the last index tried is n."""
        ins = self.disasm(where)
        for (_, mn, ops, _), (_, nxt, _, _) in zip(ins, ins[1:]):
            m = re.match(r"\$(0x[0-9a-f]+),", ops)
            if mn.startswith("cmp") and m and nxt == "jbe":
                return int(m.group(1), 16) + 1
        return None

    def call_targets(self, where):
        """[(addr, callee)] in code order; callee is a symbol name, or the
        hex target when it is no symbol's start (a local function)."""
        out = []
        ins = self.disasm(where)
        for i, (a, mn, ops, rel) in enumerate(ins):
            if mn != "call":
                continue
            # The PIC thunk: `call x; add $imm,%ebx` loads the GOT.
            if i + 1 < len(ins) and ins[i + 1][1] == "add" and ins[i + 1][2].endswith(",%ebx"):
                continue
            if rel and rel[0] == "R_386_PC32":
                out.append((a, rel[1]))
                continue
            m = re.match(r"([0-9a-f]+) <([^@>+]+)(@plt)?(@@Base)?(\+0x[0-9a-f]+)?>", ops)
            if m and not m.group(5):
                out.append((a, m.group(2)))
            elif m:
                out.append((a, f"{int(m.group(1), 16):#x}"))
        return out

    # --- which module ------------------------------------------------------

    def flavour(self):
        """'uo', '1.5' or '1.1'. UO is the only one with vehicle nodes; 1.5
        is the PIC build of the stock game."""
        if "GScr_AddFieldsForVehicleNode" in self.syms:
            return "uo"
        return "1.5" if self.sec(".rel.dyn") else "1.1"
