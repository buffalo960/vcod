#!/usr/bin/env python3
"""Diff two netfield tables by field name: wire index, struct offset and bits.

Each side is `<binary> <va> <count>`, read through netfields.py, so PE and ELF
both work. Prints fields only one side has, fields whose bits changed, and
whether the shared fields keep their wire order.

    python3 tools/re/netfield_diff.py cod_lnxded 0x080d1760 59 \
        coduo_lnxded 0x080e11a0 60
"""
import os
import subprocess
import sys


def table(binary, va, count):
    tool = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'netfields.py')
    out = subprocess.run([sys.executable, '-I', tool, binary, va, count],
                         capture_output=True, text=True, check=True).stdout
    rows = []
    for line in out.splitlines()[1:]:
        idx, name, off, bits = line.split()
        rows.append((name, int(off[4:]), int(bits[5:])))
    return rows


def main():
    a = table(*sys.argv[1:4])
    b = table(*sys.argv[4:7])
    ia = {n: (i, o, bt) for i, (n, o, bt) in enumerate(a)}
    ib = {n: (i, o, bt) for i, (n, o, bt) in enumerate(b)}
    print(f"A: {len(a)} fields, B: {len(b)} fields")
    for n, (i, o, bt) in ib.items():
        if n not in ia:
            print(f"  only B  [{i:3}] {n:24} off={o:<5} bits={bt}")
    for n, (i, o, bt) in ia.items():
        if n not in ib:
            print(f"  only A  [{i:3}] {n:24} off={o:<5} bits={bt}")
    for n, (i, o, bt) in ib.items():
        if n in ia and ia[n][2] != bt:
            print(f"  bits    {n:24} A[{ia[n][0]}]={ia[n][2]}  B[{i}]={bt}")
    shared_a = [n for n, _, _ in a if n in ib]
    shared_b = [n for n, _, _ in b if n in ia]
    if shared_a == shared_b:
        print(f"  order: the {len(shared_a)} shared fields keep their relative order")
    else:
        moved = [n for x, n in zip(shared_a, shared_b) if x != n]
        print(f"  order: {len(moved)} of {len(shared_a)} shared slots hold a different field")
        for k, (x, y) in enumerate(zip(shared_a, shared_b)):
            if x != y:
                print(f"    shared slot {k}: A {x}, B {y}")


if __name__ == '__main__':
    main()
