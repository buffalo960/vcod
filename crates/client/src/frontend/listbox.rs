//! A vertical list box's scroll bar, as `ui_mp_x86.dll` 1.1 draws and works
//! it (docs/research/cod11-front-end.md, "List box scroll bars"): the
//! arrows, the bar and the thumb on the right edge, a click on each part,
//! the thumb's drag and the keys over the list.

use vcod_common::ui_menu::UiItem;
use winit::keyboard::KeyCode;

use super::{Painter, SCROLLBAR_SIZE};
use crate::hud::HudQuad;

const ARROW_UP: &str = "ui/assets/scrollbar_arrow_up_a.tga";
const ARROW_DOWN: &str = "ui/assets/scrollbar_arrow_dwn_a.tga";
const BAR: &str = "ui/assets/scrollbar.tga";
const THUMB: &str = "ui/assets/scrollbar_thumb.tga";

/// The part of the bar under a point (`Item_ListBox_OverLB`, 0x40011840:
/// the window flags 0x800, 0x1000, 0x2000, 0x4000, 0x8000).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Part {
    Up,
    Down,
    Thumb,
    PageUp,
    PageDown,
}

/// Rows a page holds: `rect.h / elementHeight` truncated (0x40011e1b).
pub(super) fn view_max(item: &UiItem) -> usize {
    (item.rect[3] / item.element_height.max(1.0)).max(0.0) as usize
}

/// `Item_ListBox_MaxScroll` (0x400115a0): the last first row, one past a
/// full last page.
pub(super) fn max_scroll(item: &UiItem, len: usize) -> usize {
    (len + 1).saturating_sub(view_max(item))
}

/// `Item_ListBox_ThumbPosition` (0x400115f0): the thumb's top.
fn thumb_y(item: &UiItem, len: usize, top: usize) -> f32 {
    let max = max_scroll(item, len);
    let step = if max > 0 {
        (item.rect[3] - 2.0 * SCROLLBAR_SIZE - 2.0 - SCROLLBAR_SIZE) / max as f32
    } else {
        0.0
    };
    (item.rect[1] + 1.0 + SCROLLBAR_SIZE + step * top as f32).trunc()
}

fn inside(r: [f32; 4], p: [f32; 2]) -> bool {
    super::contains(r, p)
}

/// `Item_ListBox_OverLB` for a vertical list.
pub(super) fn part_at(item: &UiItem, len: usize, top: usize, p: [f32; 2]) -> Option<Part> {
    let [x, y, w, h] = item.rect;
    let bx = x + w - SCROLLBAR_SIZE;
    let s = SCROLLBAR_SIZE;
    let thumb = thumb_y(item, len, top);
    [
        (Part::Up, [bx, y, s, s]),
        (Part::Down, [bx, y + h - s, s, s]),
        (Part::Thumb, [bx, thumb, s, s]),
        (Part::PageUp, [bx, y + s, s, thumb - (y + s)]),
        (Part::PageDown, [bx, thumb + s, s, y + h - s - (thumb + s)]),
    ]
    .into_iter()
    .find(|&(_, r)| inside(r, p))
    .map(|(part, _)| part)
}

/// A click on `part` (`Item_ListBox_HandleKey`, 0x40011f67): the first row
/// after it. The thumb only starts a drag.
pub(super) fn click(item: &UiItem, len: usize, top: usize, part: Part) -> usize {
    let (max, page) = (max_scroll(item, len), view_max(item));
    match part {
        Part::Up => top.saturating_sub(1),
        Part::Down => (top + 1).min(max),
        Part::PageUp => top.saturating_sub(page),
        Part::PageDown => (top + page).min(max),
        Part::Thumb => top,
    }
}

/// The first row for the thumb dragged to `cursor_y`. INFERRED from RTCW's
/// `Scroll_ListBox_ThumbFunc`; the 1.1 capture function was not read.
pub(super) fn drag(item: &UiItem, len: usize, cursor_y: f32) -> usize {
    let start = item.rect[1] + SCROLLBAR_SIZE + 1.0;
    let span = item.rect[3] - 2.0 * SCROLLBAR_SIZE - 2.0 - SCROLLBAR_SIZE;
    let max = max_scroll(item, len);
    if span <= 0.0 {
        return 0;
    }
    let pos = (cursor_y - start - SCROLLBAR_SIZE / 2.0) * max as f32 / span;
    (pos.max(0.0) as usize).min(max)
}

/// A key over a list with `len` rows (`Item_ListBox_HandleKey`'s vertical
/// half): the arrows step the selection, Home and End jump to the ends,
/// Page Up and Down move it a page; the first row follows so the selection
/// stays in view. `None` when the key is not the list's.
pub(super) fn key(
    item: &UiItem,
    len: usize,
    top: usize,
    sel: usize,
    code: KeyCode,
) -> Option<(usize, usize)> {
    if len == 0 {
        return None;
    }
    let page = view_max(item);
    let sel = match code {
        KeyCode::ArrowUp => sel.saturating_sub(1),
        KeyCode::ArrowDown => (sel + 1).min(len - 1),
        KeyCode::Home => 0,
        KeyCode::End => len - 1,
        KeyCode::PageUp => sel.saturating_sub(page),
        KeyCode::PageDown => (sel + page).min(len - 1),
        _ => return None,
    };
    let mut top = if code == KeyCode::Home {
        0
    } else {
        top.min(sel)
    };
    if sel >= top + page {
        top = sel + 1 - page;
    }
    Some((top, sel))
}

/// `Item_ListBox_Paint`'s bar (0x400159b0): the up arrow, the bar, the down
/// arrow, then the thumb, kept above the down arrow.
pub(super) fn paint(
    p: &Painter,
    item: &UiItem,
    len: usize,
    top: usize,
    thumb_at: Option<f32>,
    out: &mut Vec<HudQuad>,
) {
    let [x, y, w, h] = item.rect;
    let s = SCROLLBAR_SIZE;
    let x = x + w - s - 1.0;
    let mut y = y + 1.0;
    p.quad([x, y, s, s], [1.0; 4], ARROW_UP, out);
    y += s - 1.0;
    let size = h - 2.0 * s;
    p.quad([x, y, s, size + 1.0], [1.0; 4], BAR, out);
    y += size - 1.0;
    p.quad([x, y, s, s], [1.0; 4], ARROW_DOWN, out);
    let thumb = thumb_at.map_or_else(|| thumb_y(item, len, top), |t| t.trunc());
    p.quad([x, thumb.min(y - s - 1.0), s, s], [1.0; 4], THUMB, out);
}

/// `Item_ListBox_ThumbDrawPosition` (0x400116a0): while dragged, the thumb
/// sits half its size above the cursor inside the bar's travel; else where
/// the first row puts it.
pub(super) fn drag_thumb(item: &UiItem, cursor_y: f32) -> Option<f32> {
    let min = item.rect[1] + SCROLLBAR_SIZE + 1.0;
    let max = item.rect[1] + item.rect[3] - 2.0 * SCROLLBAR_SIZE - 1.0;
    let half = SCROLLBAR_SIZE / 2.0;
    (cursor_y >= min + half && cursor_y <= max + half).then_some(cursor_y - half)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stock map list: `rect 404 271 192 130` less `origin -405 -135`
    /// inside `createserver_maps` at 404 137, 20 per row.
    fn map_list() -> UiItem {
        UiItem {
            rect: [403.0, 273.0, 192.0, 130.0],
            element_height: 20.0,
            ..UiItem::default()
        }
    }

    #[test]
    fn eleven_maps_scroll_six_rows_one_past_the_last_page() {
        let item = map_list();
        assert_eq!(view_max(&item), 6);
        assert_eq!(max_scroll(&item, 11), 6);
        assert_eq!(max_scroll(&item, 4), 0);
        // Thumb at 273 + 17, travel (130 - 50) / 6 per row.
        assert_eq!(thumb_y(&item, 11, 0), 290.0);
        assert_eq!(thumb_y(&item, 11, 3), 330.0);
    }

    #[test]
    fn the_bar_parts_step_and_page_the_first_row() {
        let item = map_list();
        let bx = 403.0 + 192.0 - 8.0;
        assert_eq!(part_at(&item, 11, 0, [bx, 280.0]), Some(Part::Up));
        assert_eq!(part_at(&item, 11, 0, [bx, 400.0]), Some(Part::Down));
        assert_eq!(part_at(&item, 11, 0, [bx, 295.0]), Some(Part::Thumb));
        assert_eq!(part_at(&item, 11, 3, [bx, 300.0]), Some(Part::PageUp));
        assert_eq!(part_at(&item, 11, 0, [bx, 350.0]), Some(Part::PageDown));
        assert_eq!(part_at(&item, 11, 0, [500.0, 300.0]), None);
        assert_eq!(click(&item, 11, 0, Part::Down), 1);
        assert_eq!(click(&item, 11, 6, Part::Down), 6);
        assert_eq!(click(&item, 11, 0, Part::Up), 0);
        assert_eq!(click(&item, 11, 1, Part::PageDown), 6);
        assert_eq!(click(&item, 11, 5, Part::PageUp), 0);
        assert_eq!(drag(&item, 11, 290.0 + 8.0), 0);
        assert_eq!(drag(&item, 11, 400.0), 6);
    }

    #[test]
    fn keys_move_the_selection_and_keep_it_in_view() {
        let item = map_list();
        assert_eq!(key(&item, 11, 0, 5, KeyCode::ArrowDown), Some((1, 6)));
        assert_eq!(key(&item, 11, 1, 1, KeyCode::ArrowUp), Some((0, 0)));
        assert_eq!(key(&item, 11, 0, 0, KeyCode::End), Some((5, 10)));
        assert_eq!(key(&item, 11, 5, 10, KeyCode::Home), Some((0, 0)));
        assert_eq!(key(&item, 11, 0, 2, KeyCode::PageDown), Some((3, 8)));
        assert_eq!(key(&item, 11, 0, 0, KeyCode::KeyA), None);
    }
}
