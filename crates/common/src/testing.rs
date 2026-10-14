//! Test helpers for every crate. Always compiled because the client crate's
//! tests call into it.

use crate::pk3::Pk3Fs;
use std::path::PathBuf;

/// `$COD_DIR`, else the test binary's directory, which never holds the game.
pub fn game_dir() -> PathBuf {
    crate::game_dir::default_game_dir()
}

/// `None` when the game is not installed; callers return early so the run
/// stays green.
pub fn game_fs() -> Option<Pk3Fs> {
    let dir = game_dir().join("main");
    if !dir.is_dir() {
        return None;
    }
    Pk3Fs::open(&dir).ok()
}

/// United Offensive's `uo/` over its sibling `main/`, `None` unless the
/// install is UO's: the UO gates run only with `COD_DIR` at one.
pub fn uo_fs() -> Option<Pk3Fs> {
    let dir = game_dir().join("uo");
    if !dir.is_dir() {
        return None;
    }
    Pk3Fs::open(&dir).ok()
}

/// `maps/mp/mp_pavlov.bsp` from the retail paks.
pub fn real_bsp() -> Option<Vec<u8>> {
    game_fs()?.read("maps/mp/mp_pavlov.bsp")
}

/// A committed capture under this crate's `tests/fixtures/`, by relative path.
pub fn fixture(rel: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("fixture {}: {e}", path.display()))
}

/// A committed gamestate capture (`--save-fixture`), decoded.
pub fn fixture_gamestate(
    p: &crate::net::protocol::Protocol,
    gamestate: &str,
) -> crate::net::gamestate::Gamestate {
    use crate::net::huffman::Huffman;
    use crate::net::msg::MsgReader;
    let gs_data = fixture(gamestate);
    crate::net::gamestate::parse(&mut MsgReader::new(&gs_data[4..], &Huffman::new()), p)
        .unwrap_or_else(|e| panic!("{gamestate}: {e:?}"))
}

/// Every frame of a committed snapshot capture (the `--save-snapshots`
/// format: message number, length, message), decoded against its gamestate
/// capture, with the server commands between them.
pub fn fixture_frames(
    p: &crate::net::protocol::Protocol,
    gamestate: &str,
    snapshots: &str,
) -> Vec<Frame> {
    use crate::net::huffman::Huffman;
    use crate::net::msg::MsgReader;
    use crate::net::snapshot::{SVC_EOF, SVC_NOP, SVC_SERVER_COMMAND, SVC_SNAPSHOT, SnapshotRing};
    let h = Huffman::new();
    let gs = fixture_gamestate(p, gamestate);
    let mut ring = SnapshotRing::new();
    ring.set_baselines(gs.baselines.clone());
    let data = fixture(snapshots);
    let mut out = Vec::new();
    let mut commands = Vec::new();
    let mut off = 0usize;
    while off + 8 <= data.len() {
        let message_num = u32::from_le_bytes(data[off..off + 4].try_into().unwrap());
        let len = u32::from_le_bytes(data[off + 4..off + 8].try_into().unwrap()) as usize;
        off += 8;
        let mut r = MsgReader::new(&data[off + 4..off + len], &h);
        off += len;
        loop {
            match r.read_byte() {
                SVC_EOF => break,
                SVC_NOP => {}
                SVC_SERVER_COMMAND => {
                    r.read_long();
                    commands.push(r.read_big_string());
                }
                SVC_SNAPSHOT => {
                    let snap = ring.parse_into(&mut r, p, message_num).unwrap().clone();
                    out.push(Frame {
                        commands: std::mem::take(&mut commands),
                        snapshot: snap,
                    });
                }
                op => panic!("{snapshots}: unhandled svc op {op}"),
            }
            assert!(
                !r.is_overflowed(),
                "{snapshots}: message {message_num} overran"
            );
        }
    }
    out
}

/// One frame of [`fixture_frames`]: the server commands that arrived since
/// the last one, then the snapshot.
pub struct Frame {
    pub commands: Vec<String>,
    pub snapshot: crate::net::snapshot::Snapshot,
}
