#!/usr/bin/env python3
"""Census of the `.efx` chaining keys: which stock effects use `emitfx`,
`deathfx`, `impactfx`, `playfx`, the `relative` flag and `useBBox`, how
deep the chains run, and which effects vcod's event table reaches
(docs/research/efx-grammar.md R10).

Unzip the `fx/` tree of the paks into a scratch directory first (grammar
doc section 0), then:

    tools/efx_chains.py <dir>              # counts, depth, cycles
    tools/efx_chains.py <dir> -v           # every entry
    tools/efx_chains.py <dir> --working    # the event-reachable closure

The working set is `fx/iw_impacts.csv` plus `fx/muzzleflashes/` and
`fx/shellejects/`.
"""
import collections
import glob
import os
import re
import sys

LISTS = ('emitfx', 'deathfx', 'impactfx', 'playfx')


def norm(name):
    # 0x492eb0 keeps a list entry up to its first '.'.
    return name.lower().replace('\\', '/').split('.')[0] + '.efx'


def scan(root):
    res = collections.defaultdict(list)
    graph = {}
    for dp, _, fs in os.walk(os.path.join(root, 'fx')):
        for f in fs:
            if not f.endswith('.efx'):
                continue
            p = os.path.join(dp, f)
            rel = os.path.relpath(p, root).replace('\\', '/').lower()
            text = open(p, encoding='latin-1').read().replace('\r', '')
            block, lst, edges = None, None, []
            for ln in text.split('\n'):
                s = ln.strip()
                if not s or s.startswith('//'):
                    continue
                if not ln.startswith(('\t', ' ')) and s not in '{}[]':
                    block = s
                    continue
                if lst is not None:
                    if s == ']':
                        lst = None
                    elif s != '[':
                        res[lst].append((rel, block, s))
                        edges.append(s)
                    continue
                key = s.split()[0]
                if key in LISTS:
                    lst = key
                elif key == 'flags':
                    for flag in ('relative', 'useBBox'):
                        if re.search(r'\b%s\b' % flag, s):
                            res[flag].append((rel, block, s))
            graph[rel] = [norm(e) for e in edges]
    return res, graph


def depth(graph, f, stack=frozenset()):
    if f in stack:
        return float('inf')
    return max((1 + depth(graph, c, stack | {f}) for c in graph.get(f, [])), default=0)


def working_set(root, graph):
    roots = set()
    csv = open(os.path.join(root, 'fx/iw_impacts.csv'), encoding='latin-1').read()
    for cell in re.split(r'[,\n]', csv.replace('\r', '')):
        cell = cell.strip().strip('"')
        if cell.lower().startswith('fx/'):
            roots.add(norm(cell))
    for d in ('fx/muzzleflashes', 'fx/shellejects'):
        for p in glob.glob(os.path.join(root, d, '*.efx')):
            roots.add(os.path.relpath(p, root).replace('\\', '/').lower())
    seen, todo = set(), list(roots)
    while todo:
        f = todo.pop()
        if f not in seen:
            seen.add(f)
            todo.extend(graph.get(f, []))
    return roots, seen


def main():
    root = sys.argv[1]
    res, graph = scan(root)
    for k in LISTS + ('relative', 'useBBox'):
        files = sorted({r[0] for r in res[k]})
        print(f'{k}: {len(res[k])} entries in {len(files)} files')
        if '-v' in sys.argv:
            for r in res[k]:
                print('   ', *r)
    d = {f: depth(graph, f) for f in graph}
    finite = [v for v in d.values() if v != float('inf')]
    print('deepest finite chain', max(finite))
    print('cycles', sorted(f for f, v in d.items() if v == float('inf')))
    missing = sorted({c for cs in graph.values() for c in cs if c not in graph})
    print('missing targets', missing)
    if '--working' in sys.argv:
        roots, seen = working_set(root, graph)
        print(f'working set {len(roots)} effects, {len(seen)} with their chains')
        for k in LISTS + ('relative',):
            print(f'  {k}:', ' '.join(sorted({r[0] for r in res[k] if r[0] in seen})))


if __name__ == '__main__':
    main()
