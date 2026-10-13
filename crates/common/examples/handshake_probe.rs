//! Walks a retail server's handshake at a given protocol number and prints
//! what comes back: the out-of-band replies, the gamestate's configstrings
//! and, given United Offensive's netfield tables, its baselines and the
//! snapshots that follow a spectator's entry into the world. vcod's own
//! decoder has no UO tables; this is the evidence for
//! docs/research/coduo-diff.md, written against the codec that doc describes:
//! a table-select bit after an entity's number, bits -100 a 16-bit angle, a
//! 7-bit usercmd weapon, 63 HUD elements of 30 fields.
//!
//!   cargo run -p vcod-common --example handshake_probe -- 127.0.0.1:30811 22 \
//!       [tables_dir [saved.bin]]
//!
//! `tables_dir` holds `tools/re/netfields.py <bin> <va> <count>` dumps named
//! ent.txt, alt.txt, ps.txt, cli.txt, obj.txt and hud.txt (the addresses are
//! in the doc). `saved.bin` gets the gamestate message as it came off the
//! netchan; with the address `-` it is read back instead of connecting.
use std::collections::{BTreeMap, HashMap};
use std::net::UdpSocket;
use std::time::{Duration, Instant};
use vcod_common::net::com_hash_key;
use vcod_common::net::connectionless::{build_connect, build_oob, parse_oob};
use vcod_common::net::huffman::Huffman;
use vcod_common::net::msg::{
    CLC_BITS, CLC_EOF, CLC_MOVE_NO_DELTA, MsgReader, MsgWriter, SVC_BASELINE, SVC_CONFIGSTRING,
    SVC_EOF, SVC_GAMESTATE, SVC_NOP, SVC_SERVER_COMMAND, SVC_SNAPSHOT,
};
use vcod_common::net::netchan::Netchan;

type Table = Vec<(String, i32)>;

struct Tables {
    ent: [Table; 2],
    ps: Table,
    cli: Table,
    obj: Table,
    hud: Table,
}

/// `(name, bits)` rows of one netfields.py table dump.
fn load_table(path: &std::path::Path) -> anyhow::Result<Table> {
    let mut rows = Vec::new();
    for line in std::fs::read_to_string(path)?.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() == 4 && f[0].parse::<usize>().is_ok() {
            rows.push((f[1].to_string(), f[3].trim_start_matches("bits=").parse()?));
        }
    }
    Ok(rows)
}

fn load_tables(dir: &str) -> anyhow::Result<Tables> {
    let d = std::path::Path::new(dir);
    Ok(Tables {
        ent: [
            load_table(&d.join("ent.txt"))?,
            load_table(&d.join("alt.txt"))?,
        ],
        ps: load_table(&d.join("ps.txt"))?,
        cli: load_table(&d.join("cli.txt"))?,
        obj: load_table(&d.join("obj.txt"))?,
        hud: load_table(&d.join("hud.txt"))?,
    })
}

/// A changed field's value, rendered; `None` when the change bit is clear.
type Field = Option<String>;

/// One field of UO's struct delta (coduo_lnxded 0x8081850): change bit, a
/// zero flag, then the float selector, the 16-bit angle or the packed value.
fn read_field(r: &mut MsgReader, bits: i32) -> Field {
    if r.read_bits(1) == 0 {
        return None;
    }
    Some(match bits {
        0 if r.read_bits(1) == 0 => "0".into(),
        0 => read_float(r),
        -100 if r.read_bits(1) == 0 => "0".into(),
        -100 => read_angle(r),
        _ if r.read_bits(1) == 0 => "0".into(),
        _ => format!("{}", r.read_packed_bits(bits)),
    })
}

/// The float selector: a 13-bit integer biased by 0x1000, else raw.
fn read_float(r: &mut MsgReader) -> String {
    if r.read_bits(1) == 0 {
        format!("{}", r.read_packed_bits(13) - 0x1000)
    } else {
        format!("{}", f32::from_bits(r.read_long() as u32))
    }
}

/// An angle as a 16-bit short of 360/65536 degrees (coduo_lnxded 0x8080815).
fn read_angle(r: &mut MsgReader) -> String {
    format!("{:.2}deg", f32::from(r.read_short()) * 360.0 / 65536.0)
}

/// A playerState scalar (coduo_lnxded 0x8082826): no zero flag on floats or
/// integers, only on the -100 angles.
fn read_ps_field(r: &mut MsgReader, bits: i32) -> Field {
    if r.read_bits(1) == 0 {
        return None;
    }
    Some(match bits {
        0 => read_float(r),
        -100 if r.read_bits(1) == 0 => "0".into(),
        -100 => read_angle(r),
        _ => format!("{}", r.read_packed_bits(bits)),
    })
}

/// `lc` then up to `lc` fields; the changed ones as `name=value`.
fn read_fields(r: &mut MsgReader, table: &Table, ps: bool) -> Vec<String> {
    let lc = (r.read_byte() as usize).min(table.len());
    let mut out = Vec::new();
    for (name, bits) in table.iter().take(lc) {
        let v = if ps {
            read_ps_field(r, *bits)
        } else {
            read_field(r, *bits)
        };
        if let Some(v) = v {
            out.push(format!("{name}={v}"));
        }
    }
    out
}

/// UO's entity delta after the number: table-select bit (eType 12), removed,
/// changed, then the fields. `None` for a remove.
fn read_entity(r: &mut MsgReader, t: &Tables) -> Option<(u8, Vec<String>)> {
    let alt = r.read_bits(1) as u8;
    if r.read_bits(1) == 1 {
        return None;
    }
    if r.read_bits(1) == 0 {
        return Some((alt, Vec::new()));
    }
    Some((alt, read_fields(r, &t.ent[alt as usize], false)))
}

/// The array blocks after the playerState scalars (coduo_lnxded 0x8082826).
/// Returns the HUD element counts.
fn read_ps_arrays(r: &mut MsgReader, t: &Tables) -> (i32, i32) {
    const STAT_BITS: [i32; 6] = [16, 16, 16, 6, 16, 8];
    if r.read_bits(1) == 1 {
        let mask = r.read_bits(6);
        for (i, bits) in STAT_BITS.iter().enumerate() {
            if mask & (1 << i) != 0 {
                match bits {
                    16 => {
                        r.read_short();
                    }
                    8 => {
                        r.read_byte();
                    }
                    b => {
                        r.read_bits(*b);
                    }
                }
            }
        }
    }
    let shorts = |r: &mut MsgReader| {
        for _ in 0..4 {
            if r.read_bits(1) == 1 {
                let mask = r.read_short() as u16;
                for _ in 0..mask.count_ones() {
                    r.read_short();
                }
            }
        }
    };
    if r.read_bits(1) == 1 {
        shorts(r); // ammo, behind a group gate
    }
    shorts(r); // ammoclip, no group gate
    if r.read_bits(1) == 1 {
        for _ in 0..16 {
            r.read_bits(3);
            if r.read_bits(1) == 1 {
                for (_, bits) in &t.obj {
                    read_field(r, *bits);
                }
            }
        }
    }
    let mut counts = (0, 0);
    if r.read_bits(1) == 1 {
        for count in [&mut counts.0, &mut counts.1] {
            *count = r.read_bits(6);
            for _ in 0..*count {
                let last = r.read_bits(5) as usize;
                for (_, bits) in t.hud.iter().take(last + 1) {
                    read_field(r, *bits);
                }
            }
        }
    }
    counts
}

/// What the snapshots need out of the gamestate.
#[derive(Default)]
struct Joined {
    checksum_feed: i32,
    server_id: i32,
    /// Each baseline's eType, for entities whose delta leaves it alone.
    base_etype: HashMap<i32, u8>,
}

fn etype(fields: &[String]) -> Option<u8> {
    fields
        .iter()
        .find_map(|s| s.strip_prefix("eType="))
        .and_then(|v| v.parse().ok())
}

fn dump_gamestate(msg: &[u8], t: Option<&Tables>) -> Joined {
    let huff = Huffman::new();
    let mut r = MsgReader::new(&msg[4..], &huff);
    let mut j = Joined::default();
    loop {
        match r.read_byte() {
            SVC_SERVER_COMMAND => {
                let seq = r.read_long();
                println!("svc_serverCommand {seq} {:?}", r.read_big_string());
            }
            SVC_GAMESTATE => println!("svc_gamestate seq {}", r.read_long()),
            SVC_CONFIGSTRING => {
                let i = r.read_short();
                let s = r.read_big_string();
                if i == 1 {
                    j.server_id = vcod_common::net::info_value_for_key(&s, "sv_serverid")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                }
                println!("cs {i} {s:?}");
            }
            SVC_BASELINE => {
                let Some(t) = t else {
                    println!("svc_baseline at bit {}; no tables, stopping", r.bits_read());
                    return j;
                };
                let num = r.read_bits(10);
                match read_entity(&mut r, t) {
                    Some((alt, f)) => {
                        j.base_etype.insert(num, etype(&f).unwrap_or(0));
                        println!("baseline {num} alt {alt}: {}", f.join(" "));
                    }
                    None => println!("baseline {num} removed"),
                }
            }
            SVC_EOF => {
                let client_num = r.read_long();
                let feed = r.read_long();
                println!(
                    "svc_EOF: clientNum {client_num} checksumFeed {feed}; read {} of {} bits",
                    r.bits_read(),
                    r.plain().len() * 8
                );
                j.checksum_feed = feed;
                return j;
            }
            op => {
                println!("op {op} at bit {}; stopping", r.bits_read());
                return j;
            }
        }
        if r.is_overflowed() {
            println!("overflowed");
            return j;
        }
    }
}

/// One sequenced message after the gamestate. Server commands land in `cmds`
/// (the usercmd key hashes them).
fn dump_message(msg: &[u8], t: &Tables, j: &Joined, cmds: &mut [String], last_cmd: &mut i32) {
    let huff = Huffman::new();
    let mut r = MsgReader::new(&msg[4..], &huff);
    loop {
        match r.read_byte() {
            SVC_NOP => {}
            SVC_SERVER_COMMAND => {
                let seq = r.read_long();
                let s = r.read_big_string();
                println!("  svc_serverCommand {seq} {s:?}");
                cmds[seq as usize & 63] = s;
                *last_cmd = (*last_cmd).max(seq);
            }
            SVC_SNAPSHOT => {
                let time = r.read_long();
                let delta = r.read_byte();
                let flags = r.read_byte();
                let ps = read_fields(&mut r, &t.ps, true);
                let hud = read_ps_arrays(&mut r, t);
                let mut ents: BTreeMap<u8, usize> = BTreeMap::new();
                let mut alt = 0;
                loop {
                    let num = r.read_bits(10);
                    if num == 1023 || r.is_overflowed() {
                        break;
                    }
                    if let Some((a, f)) = read_entity(&mut r, t) {
                        alt += usize::from(a);
                        let et = etype(&f)
                            .or_else(|| j.base_etype.get(&num).copied())
                            .unwrap_or(0);
                        *ents.entry(et).or_default() += 1;
                    }
                }
                let mut clients = 0;
                while r.read_bits(1) == 1 && !r.is_overflowed() {
                    r.read_bits(6);
                    if r.read_bits(1) == 0 && r.read_bits(1) == 1 {
                        read_fields(&mut r, &t.cli, false);
                    }
                    clients += 1;
                }
                println!(
                    "  snapshot t={time} delta={delta} flags={flags} ps[{}] hud {hud:?} \
                     ents by eType {ents:?} alt {alt} clients {clients}",
                    ps.join(" ")
                );
            }
            SVC_EOF => {
                println!(
                    "  svc_EOF; read {} of {} bits",
                    r.bits_read(),
                    r.plain().len() * 8
                );
                return;
            }
            op => {
                println!("  op {op} at bit {}; stopping", r.bits_read());
                return;
            }
        }
        if r.is_overflowed() {
            println!("  overflowed");
            return;
        }
    }
}

fn oob_reply(sock: &UdpSocket) -> Option<(String, String)> {
    let mut buf = [0u8; 16384];
    let n = sock.recv(&mut buf).ok()?;
    let (cmd, rest) = parse_oob(&buf[..n])?;
    Some((cmd.to_string(), String::from_utf8_lossy(rest).into_owned()))
}

/// A client message: a `clc_moveNoDelta` of one cmd the server reads as
/// unchanged from its playerstate base (changed bit equal to `key & 1`), so
/// the cmd's fields never reach the wire.
fn move_message(chan: &mut Netchan, j: &Joined, last_cmd: i32) -> anyhow::Result<Vec<u8>> {
    let huff = Huffman::new();
    let ack = chan.incoming_sequence as i32;
    let slot = last_cmd as usize & 63;
    let key = j.checksum_feed ^ ack ^ com_hash_key(&chan.server_commands[slot], 32);
    let mut w = MsgWriter::new(&huff);
    w.write_bits(CLC_MOVE_NO_DELTA, CLC_BITS);
    w.write_byte(1);
    w.write_bits(0, 1);
    w.write_long(0);
    w.write_bits(key & 1, 1);
    w.write_bits(CLC_EOF, CLC_BITS);
    chan.build_out(j.server_id, ack, last_cmd, &w.into_ops(), &huff)
}

fn run(
    addr: &str,
    protocol: u32,
    t: Option<&Tables>,
    saved: Option<&String>,
) -> anyhow::Result<()> {
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    sock.connect(addr)?;
    sock.set_read_timeout(Some(Duration::from_millis(500)))?;

    sock.send(&build_oob("getchallenge"))?;
    let (cmd, rest) = oob_reply(&sock).ok_or_else(|| anyhow::anyhow!("no challenge"))?;
    println!("oob {cmd} {rest:?}");
    let challenge: i32 = rest.trim().parse()?;

    let qport: u16 = 0x2000 | (std::process::id() as u16 & 0x0fff);
    let userinfo = format!(
        "\\cg_predictItems\\1\\cl_punkbuster\\0\\cl_guid\\unknown\\cl_anonymous\\0\\handicap\\100\
         \\color\\4\\head\\default\\model\\multi\\snaps\\20\\rate\\25000\\name\\probe\
         \\protocol\\{protocol}\\qport\\{qport}\\challenge\\{challenge}"
    );
    sock.send(&build_connect(&userinfo))?;
    let (cmd, rest) = oob_reply(&sock).ok_or_else(|| anyhow::anyhow!("no connect reply"))?;
    println!("oob {cmd} {rest:?}");
    if cmd != "connectResponse" {
        return Ok(());
    }

    let huff = Huffman::new();
    let mut chan = Netchan::new(qport, challenge);
    let mut last_cmd = 0;
    let mut joined: Option<Joined> = None;
    let mut snapshots = 0;
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut buf = [0u8; 16384];
    sock.set_read_timeout(Some(Duration::from_millis(50)))?;
    'outer: while Instant::now() < deadline && snapshots < 12 {
        let pkt = match &joined {
            Some(j) => move_message(&mut chan, j, last_cmd)?,
            None => {
                let mut w = MsgWriter::new(&huff);
                w.write_bits(CLC_EOF, CLC_BITS);
                let ack = chan.incoming_sequence as i32;
                chan.build_out(0, ack, 0, &w.into_ops(), &huff)?
            }
        };
        sock.send(&pkt)?;
        // Drain everything queued before the next send.
        while let Ok(n) = sock.recv(&mut buf) {
            if let Some((cmd, rest)) = parse_oob(&buf[..n]) {
                println!("oob {cmd} {:?}", String::from_utf8_lossy(rest));
                continue;
            }
            let Some(msg) = chan.process_in(&buf[..n], &huff)? else {
                continue;
            };
            let plain = MsgReader::new(&msg[4..], &huff).plain().to_vec();
            if joined.is_none() {
                // A client not in the world gets bare svc_EOF messages until
                // the gamestate goes out.
                if !plain.contains(&SVC_GAMESTATE) {
                    continue;
                }
                if let Some(path) = saved {
                    std::fs::write(path, &msg)?;
                    println!("wrote {} bytes to {path}", msg.len());
                }
                let j = dump_gamestate(&msg, t);
                if t.is_none() {
                    break 'outer;
                }
                joined = Some(j);
                continue;
            }
            if plain.len() <= 2 {
                continue; // bare svc_EOF while the server moves us into the world
            }
            println!(
                "message {} ({} plain bytes)",
                chan.incoming_sequence,
                plain.len()
            );
            if let Some(t) = t {
                let j = joined.as_ref().unwrap();
                dump_message(&msg, t, j, &mut chan.server_commands, &mut last_cmd);
            }
            snapshots += usize::from(plain.contains(&SVC_SNAPSHOT));
        }
    }
    sock.send(&build_oob("disconnect"))?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let addr = args.get(1).map_or("127.0.0.1:30811", String::as_str);
    let protocol: u32 = args.get(2).map_or(Ok(22), |s| s.parse())?;
    let tables = args.get(3).map(|d| load_tables(d)).transpose()?;
    let saved = args.get(4);
    if addr == "-" {
        let path = saved.ok_or_else(|| anyhow::anyhow!("`-` needs a saved message"))?;
        dump_gamestate(&std::fs::read(path)?, tables.as_ref());
        return Ok(());
    }
    run(addr, protocol, tables.as_ref(), saved)
}
