//! The engine's two message windows the cgame draws: game messages (`e`,
//! `f`, `iPrintLn`) above the compass and bold messages (`c`, `g`,
//! `iPrintLnBold`, `announcement`) centred over the crosshair. CoDMP.exe
//! keeps both as console-line windows (0x408be0, 0x409920); the layout and
//! timing are in docs/research/cod11-chat.md. Kill lines (the cgame's trap
//! 4) go into the game window: docs/research/cod11-hud-protocol.md 1.1.

use super::HudQuad;
use super::font::{Slot, UiFonts};
use super::hudelem::Virtual;
use super::player::{menu_text_rgba, text_width};

/// The ring each window keeps.
const SLOTS: usize = 8;
/// How many of the oldest slots a new line forces into their fade.
const FORCED: usize = 3;
/// A new line slides the window up over this long, and fades in over it.
const SLIDE_MS: i32 = 250;
const FADE_IN_MS: i32 = 250;
const FADE_OUT_MS: i32 = 500;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// `con_gamemessagetime` 5 s, at (6, 357), lines 12 apart.
    Game,
    /// `con_boldgamemessagetime` 8 s, centred on x 320 above y 180, lines 16
    /// apart.
    Bold,
}

impl Kind {
    fn life_ms(self) -> i32 {
        match self {
            Kind::Game => 5000,
            Kind::Bold => 8000,
        }
    }

    fn line_height(self) -> f32 {
        match self {
            Kind::Game => 12.0,
            Kind::Bold => 16.0,
        }
    }

    /// Font 0 at 0.25, or font 4 at a third.
    fn font<'a>(self, fonts: &'a UiFonts, v: &Virtual) -> (&'a super::font::Font, f32) {
        let (slot, scale) = match self {
            Kind::Game => (Slot::Default, 0.25),
            Kind::Bold => (Slot::BigFixed, 1.0 / 3.0),
        };
        (fonts.pick(slot, scale, 480.0 * v.scale), scale)
    }
}

/// One obituary as trap 4 writes it: the attacker's name and a space in
/// its team colour, the icon, then a space and the victim's name in theirs.
/// Names are already `killfeed::line_name`d.
#[derive(Clone, Debug, PartialEq)]
pub struct KillLine {
    pub attacker: Option<(String, [f32; 3])>,
    pub icon: String,
    /// The weapon's `wideKillIcon`: trap 4's width 2.8, else 1.4.
    pub icon_wide: bool,
    pub victim: (String, [f32; 3]),
}

/// The icon's width and height codes, trap 4's 1.4 and 2.8 times 32
/// truncated (CoDMP.exe 0x4021fd).
const ICON_CODE: i32 = 44;
const WIDE_ICON_CODE: i32 = 89;

#[derive(Clone)]
enum Body {
    Text(String),
    Kill(KillLine),
}

#[derive(Clone)]
struct Line {
    body: Body,
    start: i32,
    end: i32,
}

/// One window's ring. A slot with `start` 0 is empty.
pub struct Window {
    kind: Kind,
    slots: [Option<Line>; SLOTS],
    current: usize,
}

impl Window {
    pub fn new(kind: Kind) -> Window {
        Window {
            kind,
            slots: Default::default(),
            current: 0,
        }
    }

    /// A localized message, one line per `\n`.
    pub fn push(&mut self, text: &str, now_ms: i32) {
        for line in text.split('\n').filter(|l| !l.is_empty()) {
            self.add_line(Body::Text(line.to_string()), now_ms);
        }
    }

    /// A kill line, on `con_gamemessagetime` like any game message.
    pub fn push_kill(&mut self, line: KillLine, now_ms: i32) {
        self.add_line(Body::Kill(line), now_ms);
    }

    #[cfg(test)]
    pub fn kill_lines(&self) -> Vec<&KillLine> {
        self.slots
            .iter()
            .flatten()
            .filter_map(|l| match &l.body {
                Body::Kill(k) => Some(k),
                Body::Text(_) => None,
            })
            .collect()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    pub fn clear(&mut self) {
        self.slots = Default::default();
        self.current = 0;
    }

    /// `0x408be0`: the line takes the current slot, and the three oldest
    /// that would outlive the next 500 ms start their fade now.
    fn add_line(&mut self, body: Body, now: i32) {
        self.slots[self.current] = Some(Line {
            body,
            start: now,
            end: now + self.kind.life_ms(),
        });
        self.current = (self.current + 1) % SLOTS;
        for i in 0..FORCED {
            if let Some(l) = &mut self.slots[(self.current + i) % SLOTS]
                && now < l.end - FADE_OUT_MS
            {
                l.start += FADE_OUT_MS - l.end + now;
                l.end = now + FADE_OUT_MS;
            }
        }
    }

    /// `0x409920`: newest at the bottom, each older line one line height
    /// above, the stack pushed down while a new line slides in.
    pub fn build(&mut self, fonts: &UiFonts, now: i32, v: &Virtual, out: &mut Vec<HudQuad>) {
        let h = self.kind.line_height();
        let (x0, mut y) = match self.kind {
            Kind::Game => (6.0, 357.0),
            Kind::Bold => (320.0, 180.0),
        };
        for l in self.slots.iter().flatten() {
            if (0..SLIDE_MS).contains(&(now - l.start)) {
                y += ((1.0 - (now - l.start) as f32 / SLIDE_MS as f32) * h).round();
            }
        }
        let (font, scale) = self.kind.font(fonts, v);
        for k in 1..=SLOTS {
            let slot = &mut self.slots[(self.current + SLOTS - k) % SLOTS];
            let Some(l) = slot else { continue };
            if now >= l.end {
                *slot = None;
                continue;
            }
            let age = now - l.start;
            let alpha = if age < FADE_IN_MS {
                age.max(0) as f32 / FADE_IN_MS as f32
            } else if l.end - now < FADE_OUT_MS {
                (l.end - now) as f32 / FADE_OUT_MS as f32
            } else {
                1.0
            };
            y -= h;
            let text = match &l.body {
                Body::Text(text) => text,
                Body::Kill(k) => {
                    kill_line(k, font, scale, (x0, y + 12.0), alpha, v, out);
                    continue;
                }
            };
            let x = match self.kind {
                Kind::Game => x0,
                Kind::Bold => x0 - (text_width(font, text, scale) / 2.0).trunc(),
            };
            menu_text_rgba(
                font,
                text,
                (x, y + 12.0),
                scale,
                [1.0, 1.0, 1.0, alpha],
                v,
                out,
            );
        }
    }
}

/// The console-line draw's kill segments (CoDMP.exe 0x4dfa60): each text
/// run advances x by its truncated width; the icon is `scale * 48` times its
/// code over 32 on each side, its top at `baseline - (scale * 38.4 + h) / 2`,
/// white at the line's alpha.
fn kill_line(
    k: &KillLine,
    font: &super::font::Font,
    scale: f32,
    (x0, baseline): (f32, f32),
    alpha: f32,
    v: &Virtual,
    out: &mut Vec<HudQuad>,
) {
    let mut x = x0;
    let run = |s: &str, [r, g, b]: [f32; 3], x: &mut f32, out: &mut Vec<HudQuad>| {
        menu_text_rgba(font, s, (*x, baseline), scale, [r, g, b, alpha], v, out);
        *x += text_width(font, s, scale).trunc();
    };
    if let Some((name, rgb)) = &k.attacker {
        run(&format!("{name} "), *rgb, &mut x, out);
    }
    let unit = scale * 48.0 / 32.0;
    let w = unit
        * if k.icon_wide {
            WIDE_ICON_CODE
        } else {
            ICON_CODE
        } as f32;
    let h = unit * ICON_CODE as f32;
    let top = baseline - (scale * 38.4 + h) * 0.5;
    out.push(v.quad(x, top, w, h, [1.0, 1.0, 1.0, alpha], &k.icon));
    x += w;
    run(&format!(" {}", k.victim.0), k.victim.1, &mut x, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live(w: &Window) -> Vec<(String, i32, i32)> {
        w.slots
            .iter()
            .flatten()
            .map(|l| {
                let text = match &l.body {
                    Body::Text(t) => t.clone(),
                    Body::Kill(k) => k.victim.0.clone(),
                };
                (text, l.start, l.end)
            })
            .collect()
    }

    #[test]
    fn a_game_line_lives_five_seconds() {
        let mut w = Window::new(Kind::Game);
        w.push("hello", 1000);
        assert_eq!(live(&w), [("hello".to_string(), 1000, 6000)]);
    }

    /// The sixth line in a ring of eight forces the oldest into its fade.
    #[test]
    fn a_full_window_fades_its_oldest_line() {
        let mut w = Window::new(Kind::Game);
        for i in 0..6 {
            w.push(&format!("m{i}"), 1000 + i);
        }
        let first = live(&w).into_iter().find(|l| l.0 == "m0").unwrap();
        assert_eq!(first.2, 1005 + FADE_OUT_MS);
        let second = live(&w).into_iter().find(|l| l.0 == "m1").unwrap();
        assert_eq!(second.2, 1001 + 5000);
    }

    /// A kill line sits in the game window at x 6 above the compass, names
    /// in their team colours either side of a 16.5-unit icon (33.375 wide).
    #[test]
    fn a_kill_line_draws_in_the_game_window() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let fonts = UiFonts::load(&fs).unwrap();
        let v = Virtual::new((640.0, 480.0));
        let red = [1.0, 0.5, 0.5];
        let blue = [0.5, 0.5, 1.0];
        let icon = |wide| {
            let mut w = Window::new(Kind::Game);
            w.push_kill(
                KillLine {
                    attacker: Some(("Att".into(), red)),
                    icon: "gfx/hud/death_melee".into(),
                    icon_wide: wide,
                    victim: ("Vic".into(), blue),
                },
                1000,
            );
            let mut out = Vec::new();
            w.build(&fonts, 2000, &v, &mut out);
            let q = out
                .iter()
                .position(|q| q.texture == "gfx/hud/death_melee")
                .unwrap();
            (out, q)
        };
        let (out, q) = icon(false);
        let [tl, tr, br, _] = out[q].verts;
        assert!((tr[0] - tl[0] - 16.5).abs() < 1e-3, "{tl:?} {tr:?}");
        assert!((br[1] - tl[1] - 16.5).abs() < 1e-3);
        // One line: baseline 357, icon top 357 - (0.25 * 38.4 + 16.5) / 2.
        assert!((tl[1] - 343.95).abs() < 1e-3, "{tl:?}");
        assert!(tl[0] > 6.0 && tl[0] < 40.0, "{tl:?}");
        let rgb = |q: &HudQuad| [q.rgba[0], q.rgba[1], q.rgba[2]];
        assert!(out[..q].iter().any(|g| rgb(g) == red));
        assert!(out[q + 1..].iter().any(|g| rgb(g) == blue));
        assert!(
            out.iter()
                .all(|g| g.verts[0][0] > 4.0 && g.verts[0][1] > 330.0)
        );
        let (out, q) = icon(true);
        let [tl, tr, _, _] = out[q].verts;
        assert!((tr[0] - tl[0] - 33.375).abs() < 1e-3);
    }
}
