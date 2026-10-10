#!/usr/bin/env python3
"""UDP relay for tools/offlan_capture.sh: forwards between a probe on
127.0.0.1:LPORT and SERVER:PORT, and logs each datagram as
`ms dir bytes seq=N frag=0|1 oob-text` (dir c2s or s2c; seq -1 is out of band).

usage: udp_relay.py LPORT SERVER PORT LOGFILE [SECONDS]"""
import socket, sys, time, struct, select
lport, shost, sport, log = int(sys.argv[1]), sys.argv[2], int(sys.argv[3]), sys.argv[4]
cs = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); cs.bind(('127.0.0.1', lport))
ss = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); ss.bind(('0.0.0.0', 0))
peer = None; t0 = time.monotonic(); out = open(log, 'w', buffering=1)
end = t0 + float(sys.argv[5]) if len(sys.argv) > 5 else t0 + 120
while time.monotonic() < end:
    r, _, _ = select.select([cs, ss], [], [], 0.5)
    for s in r:
        data, addr = s.recvfrom(65536)
        t = (time.monotonic() - t0) * 1000
        seq = struct.unpack('<i', data[:4])[0] if len(data) >= 4 else 0
        if s is cs:
            peer = addr; ss.sendto(data, (shost, sport)); d = 'c2s'
        else:
            if peer: cs.sendto(data, peer)
            d = 's2c'
        oob = data[4:40].split(b'\n')[0].decode('latin1', 'replace') if seq == -1 else ''
        frag = 1 if (seq != -1 and seq & 0x80000000) else 0
        out.write(f"{t:.1f} {d} {len(data)} seq={seq & 0x7fffffff if seq != -1 else -1} frag={frag} {oob!r}\n")
