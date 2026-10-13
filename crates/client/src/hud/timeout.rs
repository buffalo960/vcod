//! 1.5's match timeout banner (cgame 1.5 0x300195c0, drawn right after the
//! vote): "Paused", the seconds left, and the timeout message, centred on
//! the screen while configstring 1628 holds a time
//! (docs/research/cod15-protocol-diff.md, "Match timeout").

use super::HudQuad;
use super::font::{Slot, UiFonts};
use super::hudelem::Virtual;
use super::player::{menu_text_rgba, text_height, text_width};
use vcod_common::localize::Localized;
use vcod_common::net::protocol::{CS_MATCH_TIMEOUT_TEXT_V6, CS_MATCH_TIMEOUT_V6};

/// Font 0 at half scale.
const SCALE: f32 = 0.5;

#[derive(Default)]
pub struct MatchTimeout {
    /// Local ms the timeout ends at; `None` when none runs.
    end_ms: Option<i32>,
    /// Read off a gamestate, whose time is stale: no countdown.
    from_gamestate: bool,
    /// The raw configstrings last read, to tell a change.
    seen: Option<(String, String)>,
}

impl MatchTimeout {
    /// A new gamestate: the next [`Self::update`] reads the configstrings
    /// as `CG_Init` does (0x30024290).
    pub fn on_gamestate(&mut self) {
        self.seen = None;
    }

    /// Follows configstrings 1628 and 1629. A value is the ms left as of
    /// its arrival, which `CG_ConfigStringModified` (0x3002db40) adds to
    /// `trap_Milliseconds`; one read off the gamestate shows no count.
    pub fn update(&mut self, configstrings: &[String], now_ms: i32) {
        let cs = |i: usize| configstrings.get(i).cloned().unwrap_or_default();
        let (time, text) = (cs(CS_MATCH_TIMEOUT_V6), cs(CS_MATCH_TIMEOUT_TEXT_V6));
        let fresh = self.seen.is_none();
        if self.seen.as_ref().is_some_and(|(t, _)| *t == time) {
            self.seen = Some((time, text));
            return;
        }
        // `atol`: leading digits, 0 for none.
        let digits: String = time
            .trim_start()
            .chars()
            .enumerate()
            .take_while(|&(i, c)| c.is_ascii_digit() || (i == 0 && c == '-'))
            .map(|(_, c)| c)
            .collect();
        let ms = digits.parse::<i32>().unwrap_or(0);
        self.end_ms = (ms != 0).then(|| now_ms.saturating_add(ms));
        self.from_gamestate = fresh && ms != 0;
        self.seen = Some((time, text));
    }

    /// The banner's text: `"%s: %i \n%s"`, or `"%s\n%s"` off a gamestate.
    pub fn text(&self, loc: &Localized, now_ms: i32) -> Option<String> {
        let end = self.end_ms?;
        let paused = loc.get("CGAME_PAUSED").unwrap_or("CGAME_PAUSED");
        let message = loc.message(self.seen.as_ref().map_or("", |(_, t)| t.as_str()));
        Some(if self.from_gamestate {
            format!("{paused}\n{message}")
        } else {
            let secs = ((end - now_ms) / 1000).max(0);
            format!("{paused}: {secs} \n{message}")
        })
    }

    /// Centred on the 640x480 screen in white, line by line under the first.
    pub fn build(
        &self,
        fonts: &UiFonts,
        loc: &Localized,
        now_ms: i32,
        v: &Virtual,
        out: &mut Vec<HudQuad>,
    ) {
        let Some(text) = self.text(loc, now_ms) else {
            return;
        };
        let font = fonts.pick(Slot::Default, SCALE, 480.0 * v.scale);
        let h = text_height(font, SCALE);
        let lines: Vec<&str> = text.split('\n').collect();
        let w = lines
            .iter()
            .map(|l| text_width(font, l, SCALE))
            .fold(0.0, f32::max);
        let x = (640.0 - w) * 0.5;
        let y = (480.0 - h) * 0.5;
        for (i, line) in lines.iter().enumerate() {
            let baseline = y + i as f32 * h;
            menu_text_rgba(font, line, (x, baseline), SCALE, [1.0; 4], v, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cs(time: &str, text: &str) -> Vec<String> {
        let mut cs = vec![String::new(); 2048];
        cs[CS_MATCH_TIMEOUT_V6] = time.into();
        cs[CS_MATCH_TIMEOUT_TEXT_V6] = text.into();
        cs
    }

    #[test]
    fn a_called_timeout_counts_down_on_local_time() {
        let loc = Localized::default();
        let mut t = MatchTimeout::default();
        t.update(&cs("", ""), 0);
        assert_eq!(t.text(&loc, 0), None);
        // The 1.5b server's `d 1628 15000` and `d 1629` for an axis call.
        let called = cs("15000", "PATCH_1_5_TIMEOUT_CALLED\u{14}GAME_AXIS\u{15}");
        t.update(&called, 1_000);
        let text = t.text(&loc, 4_500).unwrap();
        assert_eq!(text, "CGAME_PAUSED: 11 \nPATCH_1_5_TIMEOUT_CALLEDGAME_AXIS");
        assert_eq!(
            t.text(&loc, 60_000).as_deref(),
            Some("CGAME_PAUSED: 0 \nPATCH_1_5_TIMEOUT_CALLEDGAME_AXIS")
        );
        // The recovery restarts the count; the empty value ends it.
        t.update(&cs("5000", "PATCH_1_5_TIMEOUT_ENDING\u{15}"), 10_000);
        assert!(
            t.text(&loc, 10_000)
                .unwrap()
                .starts_with("CGAME_PAUSED: 5 \n")
        );
        t.update(&cs("", "PATCH_1_5_TIMEOUT_ENDING\u{15}"), 15_000);
        assert_eq!(t.text(&loc, 15_000), None);
    }

    #[test]
    fn a_timeout_read_off_the_gamestate_shows_no_count() {
        let loc = Localized::default();
        let mut t = MatchTimeout::default();
        t.on_gamestate();
        t.update(&cs("9000", "PATCH_1_5_TIMEOUT_ENDING\u{15}"), 0);
        assert_eq!(
            t.text(&loc, 100).as_deref(),
            Some("CGAME_PAUSED\nPATCH_1_5_TIMEOUT_ENDING")
        );
    }
}
