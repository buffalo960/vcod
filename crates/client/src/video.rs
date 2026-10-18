//! The window's video mode from `r_mode`, `r_fullscreen` and the custom
//! size, and the pick of a monitor mode for exclusive fullscreen
//! (docs/research/cod11-front-end.md, "vcod: fullscreen and wide windows").

/// CoDMP.exe's mode table (0x5716c0, 13 rows of name, width, height and
/// pixel aspect 1.0, read by `R_GetModeInfo` 0x4b0ec0).
pub const MODES: [(u32, u32); 13] = [
    (320, 240),
    (400, 300),
    (512, 384),
    (640, 480),
    (800, 600),
    (960, 720),
    (1024, 768),
    (1152, 864),
    (1280, 1024),
    (1600, 1200),
    (2048, 1536),
    (856, 480),
    (1920, 1200),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fullscreen {
    Windowed,
    /// `r_fullscreen 1`: a borderless window over the whole monitor at the
    /// desktop's own resolution.
    Borderless,
    /// `r_fullscreen 2`: the monitor switched to the mode closest to the
    /// asked size.
    Exclusive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoMode {
    /// The size `r_mode` asks for; `None` keeps the window's own.
    pub size: Option<(u32, u32)>,
    pub fullscreen: Fullscreen,
}

/// What the cvars ask for. `r_mode` -1 is the custom size, as in
/// `R_GetModeInfo`, when both its sides are set; vcod registers them 0, its
/// own window size. Any other mode past the table keeps the window's size.
pub fn video_mode(r_mode: i32, r_fullscreen: i32, custom: (i32, i32)) -> VideoMode {
    let size = match r_mode {
        -1 if custom.0 > 0 && custom.1 > 0 => Some((custom.0 as u32, custom.1 as u32)),
        m => usize::try_from(m).ok().and_then(|m| MODES.get(m).copied()),
    };
    let fullscreen = match r_fullscreen {
        0 => Fullscreen::Windowed,
        2 => Fullscreen::Exclusive,
        _ => Fullscreen::Borderless,
    };
    VideoMode { size, fullscreen }
}

/// One of a monitor's video modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MonitorMode {
    pub size: (u32, u32),
    pub refresh_mhz: u32,
    pub bit_depth: u16,
}

/// The index in `modes` of the one closest to `want`: the same size if
/// the monitor has it, else the least difference in area, then in aspect;
/// among equals the highest refresh rate, then the deepest colour.
pub fn closest_mode(want: (u32, u32), modes: &[MonitorMode]) -> Option<usize> {
    let area = |(w, h): (u32, u32)| i64::from(w) * i64::from(h);
    let aspect = |(w, h): (u32, u32)| w as f32 / h.max(1) as f32;
    let key = |m: &MonitorMode| {
        (
            (area(m.size) - area(want)).abs(),
            ((aspect(m.size) - aspect(want)).abs() * 1000.0) as i64,
            std::cmp::Reverse(m.refresh_mhz),
            std::cmp::Reverse(m.bit_depth),
        )
    };
    (0..modes.len()).min_by_key(|&i| key(&modes[i]))
}

/// Alt+Enter (CoDMP.exe 0x466e52, `WM_SYSKEYDOWN` with `VK_RETURN`): the
/// `r_fullscreen` it sets, `!r_fullscreen`, before it queues `vid_restart`.
/// Retail skips it while a map loads (`cls.state` 4).
pub fn toggled(r_fullscreen: i32) -> i32 {
    i32::from(r_fullscreen == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r_mode_reads_the_table_and_minus_one_the_custom_size() {
        assert_eq!(video_mode(3, 0, (0, 0)).size, Some((640, 480)));
        assert_eq!(video_mode(12, 0, (0, 0)).size, Some((1920, 1200)));
        assert_eq!(video_mode(13, 0, (0, 0)).size, None);
        assert_eq!(video_mode(-1, 0, (0, 0)).size, None, "the window's own");
        assert_eq!(video_mode(-1, 0, (3440, 1440)).size, Some((3440, 1440)));
        assert_eq!(video_mode(-1, 0, (2560, 0)).size, None);
        assert_eq!(video_mode(-2, 0, (2560, 1080)).size, None);
    }

    #[test]
    fn r_fullscreen_picks_windowed_borderless_or_exclusive() {
        let full = |v| video_mode(-1, v, (0, 0)).fullscreen;
        assert_eq!(full(0), Fullscreen::Windowed);
        assert_eq!(full(1), Fullscreen::Borderless);
        assert_eq!(full(2), Fullscreen::Exclusive);
        assert_eq!(full(3), Fullscreen::Borderless);
    }

    #[test]
    fn exclusive_takes_the_exact_size_at_its_best_rate_else_the_nearest() {
        let m = |w, h, hz: u32, bits| MonitorMode {
            size: (w, h),
            refresh_mhz: hz * 1000,
            bit_depth: bits,
        };
        let modes = [
            m(1920, 1080, 60, 32),
            m(2560, 1440, 60, 32),
            m(2560, 1440, 144, 32),
            m(2560, 1440, 144, 24),
            m(1280, 720, 60, 32),
            m(1600, 900, 60, 32),
            m(3440, 1440, 100, 32),
        ];
        assert_eq!(closest_mode((2560, 1440), &modes), Some(2));
        assert_eq!(closest_mode((3440, 1440), &modes), Some(6));
        // 1600x1024 is nearest 1600x900 by area.
        assert_eq!(closest_mode((1600, 1024), &modes), Some(5));
        // 1920x1200 (16:10) has 1920x1080 nearest.
        assert_eq!(closest_mode((1920, 1200), &modes), Some(0));
        assert_eq!(closest_mode((640, 480), &[]), None);
    }

    #[test]
    fn alt_enter_flips_r_fullscreen_as_a_boolean() {
        assert_eq!(toggled(0), 1);
        assert_eq!(toggled(1), 0);
        assert_eq!(toggled(2), 0);
    }
}
