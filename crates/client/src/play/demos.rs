//! Where `record` writes and `demo` reads, and the 1.5 and UO automatic
//! demo (docs/research/cod11-demos.md). The file format is
//! `vcod_common::net::demo`'s.

use std::path::{Path, PathBuf};
use vcod_common::net::demo;
use vcod_common::net::protocol::{PROTOCOL_V1, Protocol};

/// `record`'s file under `dir` (the game directory's `demos/`) and the
/// name it keeps (`clc.demoName`): `name` with the protocol's `.dm_<n>`,
/// or the first free `demoNNNN`.
pub fn record_path(dir: &Path, name: Option<&str>, p: &Protocol) -> (PathBuf, String) {
    let ext = demo::extension_number(p);
    let file = |n: &str| dir.join(format!("{n}.dm_{ext}"));
    let name = match name {
        Some(n) => n.to_string(),
        None => demo::auto_name(|n| file(n).exists()),
    };
    (file(&name), name)
}

/// `timedemo`'s count (`clc.timeDemoFrames` 0x15ef014 and
/// `clc.timeDemoStart` 0x15ef018): kept for the whole demo, across its
/// gamestates, and reset by the next.
#[derive(Default)]
pub struct TimeDemo {
    frames: i32,
    start_ms: i32,
}

impl TimeDemo {
    /// One drawn frame under `timedemo`, `now_ms` the local clock; the
    /// count this frame draws at.
    pub fn step(&mut self, now_ms: i32) -> i32 {
        if self.start_ms == 0 {
            self.start_ms = now_ms;
        }
        self.frames += 1;
        self.frames
    }

    /// `CL_DemoCompleted`'s line (CoDMP.exe 0x40e920) with `timedemo`
    /// set as the demo ends; nothing when no time has passed.
    pub fn report(&self, now_ms: i32) -> Option<String> {
        let ms = now_ms - self.start_ms;
        (ms > 0).then(|| {
            format!(
                "{} frames, {:3.1} seconds: {:3.1} fps",
                self.frames,
                ms as f64 * 0.001,
                self.frames as f64 * 1000.0 / ms as f64
            )
        })
    }
}

/// `demo <name>`'s file and the protocol it was recorded under. Retail
/// reads only its own `.dm_<n>` and appends it to a name without one; vcod
/// plays all three, so a bare name takes the first of `.dm_1`, `.dm_2`
/// and `.dm_3` that exists, searched through `dirs` in order.
pub fn find(dirs: &[PathBuf], name: &str) -> Option<(PathBuf, &'static Protocol)> {
    let candidates: Vec<(String, &'static Protocol)> = match demo::extension_of(name) {
        Some(n) => vec![(name.to_string(), demo::protocol_of_extension(n)?)],
        None => (1..=3)
            .filter_map(|n| Some((format!("{name}.dm_{n}"), demo::protocol_of_extension(n)?)))
            .collect(),
    };
    for dir in dirs {
        for (file, p) in &candidates {
            let path = dir.join(file);
            if path.is_file() {
                return Some((path, p));
            }
        }
    }
    None
}

/// What 1.5's and UO's `CG_ServerCommand` runs for the server's `C` and
/// `D` (cgame_mp15.dll 0x3002f660, cases 0x43 and 0x44) while
/// `cg_autodemo` is 1: `record <name>-<yyyyMMdd-HHmmss>`, the player's
/// name kept to letters and digits (0x3002f470) and the local date and
/// time off engine trap 0xf4 (CoDMP15.exe 0x46af40), and `stoprecord`.
/// 1.1's cgame knows neither command.
pub fn auto_command(
    tokens: &[String],
    p: &Protocol,
    autodemo: &str,
    name: &str,
    stamp: &str,
) -> Option<String> {
    if p.version == PROTOCOL_V1.version || atol(autodemo) != 1 {
        return None;
    }
    match tokens.first().map(String::as_str)? {
        "C" => {
            let clean: String = name.chars().filter(char::is_ascii_alphanumeric).collect();
            Some(format!("record {clean}-{stamp}"))
        }
        "D" => Some("stoprecord".to_string()),
        _ => None,
    }
}

/// `atol`: the leading integer, 0 for none.
fn atol(s: &str) -> i64 {
    let s = s.trim_start();
    let end = s
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || (i == 0 && (c == '-' || c == '+'))))
        .map_or(s.len(), |(i, _)| i);
    s[..end].parse().unwrap_or(0)
}

/// Trap 0xf4's string, `yyyyMMdd-HHmmss` in local time.
pub fn stamp_now() -> String {
    jiff::Zoned::now().strftime("%Y%m%d-%H%M%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vcod_common::net::protocol::{PROTOCOL_V6, PROTOCOL_V22};
    use vcod_common::net::{NetEvent, UdpTransport};

    fn toks(s: &str) -> Vec<String> {
        s.split(' ').map(str::to_string).collect()
    }

    #[test]
    fn auto_demo_runs_on_1_5_and_uo_only_and_only_at_1() {
        let stamp = "20261017-120000";
        let c = |p, v| auto_command(&toks("C"), p, v, "^1Big Joe!", stamp);
        assert_eq!(
            c(&PROTOCOL_V6, "1").as_deref(),
            Some("record 1BigJoe-20261017-120000")
        );
        // UO's killcam sends `C <ms>`, which reads the same.
        assert_eq!(
            auto_command(&toks("C 5000"), &PROTOCOL_V22, "1", "a", stamp).as_deref(),
            Some("record a-20261017-120000")
        );
        assert_eq!(c(&PROTOCOL_V1, "1"), None);
        assert_eq!(c(&PROTOCOL_V6, "0"), None);
        assert_eq!(c(&PROTOCOL_V6, "2"), None);
        assert_eq!(
            auto_command(&toks("D"), &PROTOCOL_V6, "1", "a", stamp).as_deref(),
            Some("stoprecord")
        );
    }

    #[test]
    fn timedemo_reports_frames_seconds_and_rate() {
        let mut t = TimeDemo::default();
        assert_eq!(t.step(2000), 1);
        for _ in 0..99 {
            t.step(2500);
        }
        assert_eq!(
            t.report(4000).as_deref(),
            Some("100 frames, 2.0 seconds: 50.0 fps")
        );
        assert_eq!(t.report(2000), None);
    }

    #[test]
    fn record_names_count_up_and_demo_finds_any_protocol() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().to_path_buf();
        let path = |name, p| record_path(&d, name, p).0;
        assert_eq!(
            record_path(&d, None, &PROTOCOL_V1),
            (d.join("demo0000.dm_1"), "demo0000".to_string())
        );
        std::fs::write(d.join("demo0000.dm_1"), b"").unwrap();
        assert_eq!(path(None, &PROTOCOL_V1), d.join("demo0001.dm_1"));
        // The count is per extension, as the file name is.
        assert_eq!(path(None, &PROTOCOL_V22), d.join("demo0000.dm_3"));
        assert_eq!(
            record_path(&d, Some("x"), &PROTOCOL_V6),
            (d.join("x.dm_2"), "x".to_string())
        );

        std::fs::write(d.join("x.dm_3"), b"").unwrap();
        let found = find(std::slice::from_ref(&d), "x").unwrap();
        assert_eq!((found.0, found.1.version), (d.join("x.dm_3"), 22));
        let found = find(std::slice::from_ref(&d), "demo0000.dm_1").unwrap();
        assert_eq!(found.1.version, 1);
        assert!(find(&[d], "nope").is_none());
    }

    /// A retail capture played as a demo at 60 fps through the clock the
    /// online frame runs: every frame read, each in its turn, and the drawn
    /// time always between two snapshots, never past the newest read.
    /// The retail 400-frame capture as a demo, its gamestate read.
    fn retail_demo(now: std::time::Instant) -> vcod_common::net::NetClient<UdpTransport> {
        let gs = vcod_common::testing::fixture("net/gamestate-delta.bin");
        let snaps = vcod_common::testing::fixture("net/snapshots-delta.bin");
        let first = u32::from_le_bytes(snaps[..4].try_into().unwrap());
        let mut w = demo::DemoWriter::new(Vec::new());
        w.write_message(first - 1, &gs).unwrap();
        let mut data = w.finish().unwrap();
        data.truncate(data.len() - 8);
        data.extend_from_slice(&snaps);
        data.extend_from_slice(&[0xff; 8]);
        let mut net =
            vcod_common::net::NetClient::play_demo(UdpTransport::none(), now, &PROTOCOL_V1, data);
        assert!(net.pump_demo(now, None).contains(&NetEvent::GamestateReady));
        net
    }

    /// `timedemo` reads a 20 Hz demo one snapshot a frame, however fast
    /// the frames come.
    #[test]
    fn timedemo_reads_a_snapshot_a_frame() {
        use super::super::clock::{DemoClock, ServerClock};
        let now = std::time::Instant::now();
        let mut net = retail_demo(now);
        let mut clock = ServerClock::default();
        let mut td = TimeDemo::default();
        let mut frames = 0;
        let mut first = None;
        loop {
            let realtime = 1000 + frames;
            let how = DemoClock {
                freeze: false,
                timedemo_frames: clock.started().then(|| td.step(realtime)),
            };
            let until = clock.time_at(realtime, 0, how).unwrap_or(i32::MIN);
            let before = net.snapshots().newest().map(|s| s.message_num);
            let events = net.pump_demo(now, Some(until));
            if events.iter().any(|e| matches!(e, NetEvent::DemoEnded(_))) {
                break;
            }
            let newest = net.snapshots().newest().unwrap();
            // Read one snapshot past the frame's time, which steps 50 ms.
            assert!(newest.server_time > until);
            if before.is_some() {
                assert_eq!(newest.server_time, until + 50, "frame {frames}");
            }
            first.get_or_insert(newest.server_time);
            clock.update_demo(realtime, newest.server_time, 0, how);
            frames += 1;
        }
        let span = net.snapshots().newest().unwrap().server_time - first.unwrap();
        // The first frame reads to the first snapshot; the one that finds
        // the trailer is counted but not drawn.
        assert_eq!(frames, span / 50);
        assert!(
            td.report(2000)
                .unwrap()
                .starts_with(&format!("{frames} frames"))
        );
    }

    #[test]
    fn a_demo_reads_one_snapshot_ahead_of_the_clock() {
        use std::time::Instant;
        let now = Instant::now();
        let mut net = retail_demo(now);
        let mut clock = super::super::clock::ServerClock::default();
        let mut seen = std::collections::BTreeSet::new();
        let mut ended = false;
        for frame in 0..10_000 {
            let realtime = 1000 + frame * 16;
            let run = super::super::clock::DemoClock::default();
            let until = clock.time_at(realtime, 0, run).unwrap_or(i32::MIN);
            let events = net.pump_demo(now, Some(until));
            if events.iter().any(|e| matches!(e, NetEvent::DemoEnded(_))) {
                ended = true;
                break;
            }
            let newest = net.snapshots().newest().expect("a snapshot");
            seen.insert(newest.message_num);
            let t = clock.update_demo(realtime, newest.server_time, 0, run);
            assert!(
                newest.server_time > t || frame == 0,
                "frame {frame}: drawn past the newest"
            );
            let (a, b) = net.snapshots().two_for_time(t).unwrap();
            assert!(a.server_time <= t && t <= b.server_time, "frame {frame}");
        }
        assert!(ended, "the demo never ended");
        assert_eq!(seen.len(), 400);
        assert_eq!(clock.stats.resets + clock.stats.fast, 0);
    }
}
