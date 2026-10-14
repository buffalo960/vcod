//! `--save-legs`: the walk behind the moving-body pose capture. Waits for
//! `client-probes/probe_legs` to put the player on its mark on mp_carentan,
//! then runs, strafes and crouch-walks a fixed script at world yaw 0, and
//! writes every usercmd and every snapshot's movement and anim fields to
//! `crates/server/tests/fixtures/movers/<map>-dm-legs-client.txt`, which
//! `legs_ab.rs` replays cmd for cmd beside the gsc's bone log.

use std::time::Instant;
use vcod_common::net;
use vcod_common::net::msg::{NULL_USERCMD, UserCmd, WBUTTON_CROUCH};
use vcod_common::net::snapshot::Snapshot;

/// Where `probe_legs.gsc` puts the player: mp_carentan's flat bottom floor,
/// under the garden terrain (ground at z -7), not on a street.
pub const LEGS_SPOT: [f32; 3] = [932.0, -376.0, -151.875];

/// One step of the walk: how long it holds and what it holds.
struct Step {
    label: &'static str,
    ms: u128,
    forward: i8,
    right: i8,
    crouch: bool,
}

const fn step(label: &'static str, ms: u128, forward: i8, right: i8, crouch: bool) -> Step {
    Step {
        label,
        ms,
        forward,
        right,
        crouch,
    }
}

/// From the snapshot that showed the placement. The street is clear 680
/// units either side of the mark, and each run is paired with its way back.
const SCRIPT: &[Step] = &[
    step("settle", 1500, 0, 0, false),
    step("run", 1800, 127, 0, false),
    step("stop", 1000, 0, 0, false),
    step("back", 1800, -127, 0, false),
    step("stop2", 1000, 0, 0, false),
    step("strafe_right", 1500, 0, 127, false),
    step("strafe_left", 1500, 0, -127, false),
    step("stop3", 1000, 0, 0, false),
    step("diag", 1200, 127, 127, false),
    step("diag_back", 1200, -127, -127, false),
    step("stop4", 1000, 0, 0, false),
    step("crouch", 800, 0, 0, true),
    step("crouch_run", 1500, 127, 0, true),
    step("crouch_back", 1500, -127, 0, true),
    step("crouch_stop", 800, 0, 0, true),
    step("stand", 1500, 0, 0, false),
];

#[derive(Default)]
pub struct LegsProbe {
    /// Wall clock and server time of the snapshot that showed the placement.
    placed: Option<(Instant, i32)>,
    seen: Option<u32>,
    lines: Vec<String>,
    done: bool,
}

impl LegsProbe {
    pub fn running(&self) -> bool {
        !self.done
    }

    fn ms(&self, now: Instant) -> Option<u128> {
        self.placed
            .map(|(t, _)| now.saturating_duration_since(t).as_millis())
    }

    fn step(&self, now: Instant) -> Option<&'static Step> {
        let mut ms = self.ms(now)?;
        for s in SCRIPT {
            if ms < s.ms {
                return Some(s);
            }
            ms -= s.ms;
        }
        None
    }

    /// The cmd for `now`, in world angles: `send_frame` takes the
    /// playerstate's `delta_angles` off.
    pub fn cmd(&self, now: Instant) -> UserCmd {
        let mut cmd = UserCmd {
            angles: [0, 0, 0],
            ..NULL_USERCMD
        };
        if let Some(s) = self.step(now) {
            cmd.forward = s.forward;
            cmd.right = s.right;
            if s.crouch {
                cmd.wbuttons |= WBUTTON_CROUCH;
                cmd.up = -127;
            }
        }
        cmd
    }

    /// A cmd as it went on the wire, beside the world angles it was built
    /// from.
    pub fn record(&mut self, now: Instant, sent: UserCmd, world: [i32; 3]) {
        let Some(ms) = self.ms(now) else { return };
        if self.done {
            return;
        }
        self.lines.push(format!(
            "!cmd ms={ms} st={} buttons={} wbuttons={} weapon={} up={} forward={} right={} angles={},{},{} world={},{},{}",
            sent.server_time,
            sent.buttons,
            sent.wbuttons,
            sent.weapon,
            sent.up,
            sent.forward,
            sent.right,
            sent.angles[0],
            sent.angles[1],
            sent.angles[2],
            world[0],
            world[1],
            world[2],
        ));
    }

    /// Takes the placement off the first snapshot that stands on the mark,
    /// and logs every snapshot after it. True once the script has run out
    /// and a second has passed.
    pub fn observe(&mut self, now: Instant, s: &Snapshot) -> bool {
        let p = &net::protocol::PROTOCOL_V1;
        let o = s.ps.origin(p);
        if self.placed.is_none() {
            if (o[0] - LEGS_SPOT[0]).abs() < 0.5 && (o[1] - LEGS_SPOT[1]).abs() < 0.5 {
                self.placed = Some((now, s.server_time));
                println!("legs: placed at serverTime {}", s.server_time);
            } else {
                return false;
            }
        }
        if self.seen != Some(s.message_num) {
            self.seen = Some(s.message_num);
            let v = [0, 1, 2].map(|i| s.ps.field_f32(p, &format!("velocity[{i}]")));
            let label = self.step(now).map_or("end", |st| st.label);
            self.lines.push(format!(
                "!snap msg={} t={} ct={} step={label} origin={},{},{} vel={},{},{} legs={} torso={} movementDir={} pm_flags={} view={},{}",
                s.message_num,
                s.server_time,
                s.ps.field_i32(p, "commandTime"),
                o[0],
                o[1],
                o[2],
                v[0],
                v[1],
                v[2],
                s.ps.field_i32(p, "legsAnim"),
                s.ps.field_i32(p, "torsoAnim"),
                s.ps.field_i32(p, "movementDir"),
                s.ps.field_i32(p, "pm_flags"),
                s.ps.viewangles(p)[0],
                s.ps.viewangles(p)[1],
            ));
        }
        let total: u128 = SCRIPT.iter().map(|s| s.ms).sum();
        if self.ms(now).is_some_and(|ms| ms >= total + 1000) {
            self.done = true;
        }
        self.done
    }

    /// The fixture: the recipe, then every `!cmd` and `!snap` in order.
    pub fn write(
        &self,
        configstrings: &[String],
        team: &str,
        weapon: &str,
        path_dir: &str,
        overwrite: bool,
    ) -> anyhow::Result<()> {
        let serverinfo = configstrings.first().map(String::as_str).unwrap_or("");
        let key = |k: &str| net::info_value_for_key(serverinfo, k).unwrap_or("?");
        let map = key("mapname");
        let mut out = String::new();
        out.push_str(
            "# Retail CoD 1.1d dedicated server: one player walking a fixed script, the\n\
             # client half of the moving-body pose capture.\n",
        );
        out.push_str(&format!(
            "# map {map}, gametype {}, team={team} weapon={weapon}\n",
            key("g_gametype")
        ));
        out.push_str(&format!(
            "# placed serverTime={} spot={},{},{}\n",
            self.placed.map_or(0, |p| p.1),
            LEGS_SPOT[0],
            LEGS_SPOT[1],
            LEGS_SPOT[2]
        ));
        out.push_str(
            "# client-probes/probe_legs under tools/run_probe.sh puts the player on the spot at\n\
             # level time 25000 and logs its bones per frame (the server half, -legs.txt).\n\
             # Script from the snapshot that showed the placement, world yaw 0 throughout:\n#",
        );
        for s in SCRIPT {
            out.push_str(&format!(
                " {} {}ms f{} r{}{};",
                s.label,
                s.ms,
                s.forward,
                s.right,
                if s.crouch { " crouch" } else { "" }
            ));
        }
        out.push('\n');
        out.push_str(
            "# !cmd is a usercmd as it went on the wire (angles rebased on delta_angles; world=\n\
             # the angles before), ms after the placement. !snap is the playerstate per\n\
             # snapshot: t serverTime, ct commandTime, legs/torso the wire anim values.\n",
        );
        if !self.done {
            out.push_str("# BROKEN the run ended before the script did\n");
        }
        for l in &self.lines {
            out.push_str(l);
            out.push('\n');
        }
        let path = format!("{path_dir}/{map}-dm-legs-client.txt");
        if !overwrite && std::path::Path::new(&path).exists() {
            anyhow::bail!("{path} already exists; pass --overwrite-fixture to replace it");
        }
        std::fs::write(&path, out)?;
        println!("legs: {} lines -> {path}", self.lines.len());
        Ok(())
    }
}
