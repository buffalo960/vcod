//! The server's open script menu drawn as a keyboard list: a stock
//! `.menu` file (`vcod_common::menu`) turned into localized, cvar-filtered
//! rows and glyph quads.

use std::collections::HashMap;

use vcod_common::localize::Localized;
use vcod_common::menu::Menu;
use vcod_common::pk3::Pk3Fs;

use super::HudQuad;
use super::font::{self, Font};
use crate::play::join::Join;

/// One selectable row: the localized label, the response it sends, and the
/// `execKey` (if any) that answers it directly without moving the selection.
/// The "Main Menu" tab sends nothing and opens the UI's `main` instead; a
/// quick message row sends nothing and opens the script menu `opens` names.
/// The sounds are the aliases the UI's scripts `play` (menu alias set,
/// docs/research/cod11-front-end.md section 20): `onFocus`'s when the
/// selection lands on the row, `action`'s when Enter picks it, the
/// `execKey`'s when its key does.
pub struct MenuRow {
    pub label: String,
    pub response: String,
    pub key: Option<String>,
    pub main_menu: bool,
    pub opens: Option<String>,
    pub focus_sounds: Vec<String>,
    pub action_sounds: Vec<String>,
    pub key_sounds: Vec<String>,
}

/// A menu's live state: the rows a client can pick from and which is
/// highlighted.
pub struct MenuView {
    pub title: String,
    pub background: Option<String>,
    pub rows: Vec<MenuRow>,
    pub selected: usize,
    /// The menu leaves the mouse on the view (`cl_bypassMouseInput` 1).
    pub bypass_mouse: bool,
    /// The row under the cursor, as the last mouse move left it.
    pub hover: Option<usize>,
}

/// Builds a [`MenuView`]: rows from `menu.choices(cvar)`, labels through
/// `loc.translate`, each row's `key` matched against `menu.exec_keys` by
/// its response text. A menu with no choices (the quick message menus,
/// whose items are decorations) gets one row per `execKey` instead, labelled
/// by the item before it. The selection starts on the first keyed row, since
/// unkeyed rows are tabs like the team menu's "View Map"; row 0 if none is.
pub fn view(menu: &Menu, loc: &Localized, cvar: impl Fn(&str) -> Option<String>) -> MenuView {
    let mut rows = menu
        .choices(cvar)
        .into_iter()
        .map(|item| {
            let response = item.response.clone().unwrap_or_default();
            let exec = menu
                .exec_keys
                .iter()
                .find(|k| k.response.as_ref() == Some(&response));
            MenuRow {
                label: loc.translate(&item.text).into_owned(),
                response,
                key: exec.map(|k| k.key.clone()),
                main_menu: item.opens_main(),
                opens: None,
                focus_sounds: item.focus_sounds.clone(),
                action_sounds: item.action_sounds.clone(),
                key_sounds: exec.map_or_else(Vec::new, |k| k.sounds.clone()),
            }
        })
        .collect::<Vec<MenuRow>>();
    if rows.is_empty() {
        rows = menu
            .exec_keys
            .iter()
            .map(|k| MenuRow {
                label: k
                    .label
                    .and_then(|i| menu.items.get(i))
                    .map_or_else(|| k.key.clone(), |i| loc.translate(&i.text).into_owned()),
                response: k.response.clone().unwrap_or_default(),
                key: Some(k.key.clone()),
                main_menu: false,
                opens: k.opens.clone(),
                focus_sounds: Vec::new(),
                action_sounds: k.sounds.clone(),
                key_sounds: k.sounds.clone(),
            })
            .collect();
    }
    let selected = rows.iter().position(|r| r.key.is_some()).unwrap_or(0);
    MenuView {
        title: menu.name.clone(),
        background: menu.background.clone(),
        rows,
        selected,
        bypass_mouse: menu.bypass_mouse,
        hover: None,
    }
}

impl MenuView {
    /// Wraps to the last row from the first.
    pub fn up(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        self.selected = (self.selected + self.rows.len() - 1) % self.rows.len();
    }

    /// Wraps to the first row from the last.
    pub fn down(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.rows.len();
    }

    pub fn selected_row(&self) -> Option<&MenuRow> {
        self.rows.get(self.selected)
    }

    /// The row an `execKey` of `key` answers.
    pub fn row_for_key(&self, key: &str) -> Option<usize> {
        self.rows.iter().position(|r| r.key.as_deref() == Some(key))
    }

    /// The row whose band holds window point `(x, y)` on a `screen_w` x
    /// `screen_h` window, as [`build`] lays the rows out.
    pub fn row_at(&self, x: f32, y: f32, screen_w: f32, screen_h: f32) -> Option<usize> {
        (0..self.rows.len()).find(|&i| {
            let [rx, ry, rw, rh] = row_rect(i, screen_w, screen_h);
            x >= rx && x <= rx + rw && y >= ry && y < ry + rh
        })
    }

    /// A mouse move to window point `(x, y)`: entering a row selects it, as
    /// `Item_SetFocus` gives the hovered item the focus. Returns the row it
    /// selected, so the caller plays its `onFocus` sounds; moving inside the
    /// row or off every row selects nothing.
    pub fn mouse_move(&mut self, x: f32, y: f32, screen_w: f32, screen_h: f32) -> Option<usize> {
        let row = self.row_at(x, y, screen_w, screen_h);
        if row == self.hover {
            return None;
        }
        self.hover = row;
        let row = row.filter(|&r| r != self.selected)?;
        self.selected = row;
        Some(row)
    }
}

const SELECTED_COLOR: [f32; 4] = [1.0, 0.8, 0.2, 1.0];

/// Panel size at the 640x480 reference resolution the stock window was
/// designed at.
const PANEL_W: f32 = 448.0;
const PANEL_H: f32 = 288.0;
const REF_H: f32 = 480.0;

const TITLE_Y: f32 = 24.0;
const ROWS_TOP: f32 = 72.0;
const ROW_GAP: f32 = 28.0;
const ROW_X: f32 = 24.0;

/// The panel's window rect, centred and scaled with the window's height,
/// and that scale.
fn panel(screen_w: f32, screen_h: f32) -> ([f32; 4], f32) {
    let panel_scale = screen_h / REF_H;
    let w = PANEL_W * panel_scale;
    let h = PANEL_H * panel_scale;
    let x = (screen_w - w) / 2.0;
    let y = (screen_h - h) / 2.0;
    ([x, y, w, h], panel_scale)
}

/// Row `i`'s band, which the mouse picks it by: the panel's width, from
/// the row's text top to the next row's.
fn row_rect(i: usize, screen_w: f32, screen_h: f32) -> [f32; 4] {
    let ([x, y, w, _], s) = panel(screen_w, screen_h);
    [x, y + (ROWS_TOP + ROW_GAP * i as f32) * s, w, ROW_GAP * s]
}

/// A panel centred on the screen: the background image stretched over it
/// when the menu has one, the title, then one text row per choice, the
/// selected row in yellow.
pub fn build(
    view: &MenuView,
    font: &Font,
    scale: f32,
    screen_w: f32,
    screen_h: f32,
    out: &mut Vec<HudQuad>,
) {
    let ([x, y, w, h], panel_scale) = panel(screen_w, screen_h);

    if let Some(bg) = &view.background {
        out.push(HudQuad {
            verts: [[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
            uvs: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            rgba: [1.0, 1.0, 1.0, 1.0],
            texture: bg.clone(),
        });
    }

    let text_scale = scale * panel_scale;
    font::layout(
        font,
        &view.title,
        x + ROW_X * panel_scale,
        y + TITLE_Y * panel_scale,
        text_scale,
        font::COLORS[7],
        out,
    );

    for (i, row) in view.rows.iter().enumerate() {
        let color = if i == view.selected {
            SELECTED_COLOR
        } else {
            font::COLORS[7]
        };
        let ry = row_rect(i, screen_w, screen_h)[1];
        font::layout(
            font,
            &row.label,
            x + ROW_X * panel_scale,
            ry,
            text_scale,
            color,
            out,
        );
    }
}

/// Keeps `drawn`, the menu on screen keyed by its name, in step with the one
/// `join` holds open. A menu with no `.menu` file is closed in `join` too, so
/// the handshake never waits on a menu that cannot draw.
pub fn sync(
    drawn: &mut Option<(String, MenuView)>,
    join: &mut Join,
    menus: &mut MenuCache,
    fs: &Pk3Fs,
    loc: &Localized,
    configstrings: &[String],
) {
    let Some(open) = join.open() else {
        *drawn = None;
        return;
    };
    if drawn.as_ref().is_some_and(|(name, _)| *name == open.name) {
        return;
    }
    let menu = match open.index {
        Some(_) => menus.get(fs, &open.name),
        None => menus.get_ingame(fs, &open.name),
    };
    match menu {
        Some(menu) => {
            let v = view(menu, loc, |c| join.cvars.get(c, configstrings));
            *drawn = Some((open.name.clone(), v));
        }
        None => {
            join.close();
            *drawn = None;
        }
    }
}

/// One parsed menu per stock name, lazily loaded from
/// `ui_mp/scriptmenus/<name>.menu`, tried under `cl_language`'s directory
/// first (ui_mp_x86.dll 0x40008330); a missing file caches as `None` so a
/// server naming a bad menu is only tried, and warned about, once.
#[derive(Default)]
pub struct MenuCache {
    cache: HashMap<String, Option<Menu>>,
    language: String,
}

impl MenuCache {
    /// An empty cache for `cl_language`'s value.
    pub fn new(language: &str) -> MenuCache {
        MenuCache {
            cache: HashMap::new(),
            language: language.to_string(),
        }
    }

    /// Forgets every parsed menu, for a new search path.
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    pub fn get(&mut self, fs: &Pk3Fs, name: &str) -> Option<&Menu> {
        if !self.cache.contains_key(name) {
            let path = format!("ui_mp/scriptmenus/{name}.menu");
            let parsed = self.read(fs, &path);
            if parsed.is_none() {
                log::warn!("menu: no script menu at {path}");
            }
            self.cache.insert(name.to_string(), parsed);
        }
        self.cache.get(name).unwrap().as_ref()
    }

    /// A UI menu `ui_mp/ingame.txt` loads (the quick message menu), by its
    /// menuDef name, from the first of the list's files that defines it.
    pub fn get_ingame(&mut self, fs: &Pk3Fs, name: &str) -> Option<&Menu> {
        let key = format!("ingame:{name}");
        if !self.cache.contains_key(&key) {
            let files = fs.read("ui_mp/ingame.txt").map_or_else(Vec::new, |t| {
                vcod_common::ui_menu::menu_list(&String::from_utf8_lossy(&t))
            });
            let parsed = files
                .iter()
                .filter_map(|f| self.read(fs, f))
                .find(|m| m.name.eq_ignore_ascii_case(name));
            if parsed.is_none() {
                log::warn!("menu: no menu {name} in ui_mp/ingame.txt");
            }
            self.cache.insert(key.clone(), parsed);
        }
        self.cache.get(&key).unwrap().as_ref()
    }

    /// `path` tried under `cl_language`'s directory first.
    fn read(&self, fs: &Pk3Fs, path: &str) -> Option<Menu> {
        let bytes = vcod_common::ui_menu::localized_menu_path(path, &self.language)
            .and_then(|p| fs.read(&p))
            .or_else(|| fs.read(path))?;
        let include = |p: &str| fs.read(p).map(|b| String::from_utf8_lossy(&b).into_owned());
        Some(vcod_common::menu::parse_with(
            &String::from_utf8_lossy(&bytes),
            &include,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu() -> Menu {
        vcod_common::menu::parse(
            r#"{ menuDef { name "weapon_american"
      itemDef { name "a" visible 1 text "@MPMENU_1_M1A1_CARBINE" cvartest "scr_allow_m1carbine" hideCvar { "0" } action { play "mouse_click"; scriptMenuResponse "m1carbine_mp"; } onFocus { play "mouse_over"; } }
      execKey "1" { play "key_click"; scriptMenuResponse "m1carbine_mp"; }
      itemDef { name "b" visible 1 text "Garand" action { scriptMenuResponse "m1garand_mp"; } }
    } }"#,
        )
    }

    #[test]
    fn rows_follow_choices_and_keys() {
        let v = view(&menu(), &Localized::default(), |_| None);
        assert_eq!(v.rows.len(), 2);
        assert_eq!(v.rows[0].label, "MPMENU_1_M1A1_CARBINE");
        assert_eq!(v.rows[0].key.as_deref(), Some("1"));
        assert_eq!(
            v.row_for_key("1").map(|i| v.rows[i].response.as_str()),
            Some("m1carbine_mp")
        );
        assert_eq!(
            v.row_for_key("2").map(|i| v.rows[i].response.as_str()),
            None
        );
        let row = &v.rows[0];
        assert_eq!(row.focus_sounds, ["mouse_over"]);
        assert_eq!(row.action_sounds, ["mouse_click"]);
        assert_eq!(row.key_sounds, ["key_click"]);
        assert!(v.rows[1].key_sounds.is_empty());
    }

    #[test]
    fn the_main_menu_tab_is_a_row_with_no_response() {
        let m = vcod_common::menu::parse(
            r#"{ menuDef { name "weapon_x"
      itemDef { name "button_mainmenu" visible 1 text "@MPMENU_MAIN_MENU" action { close weapon_x; open main; } }
      itemDef { name "b" visible 1 text "Garand" action { scriptMenuResponse "m1garand_mp"; } }
      execKey "1" { scriptMenuResponse "m1garand_mp"; }
    } }"#,
        );
        let mut v = view(&m, &Localized::default(), |_| None);
        assert_eq!(v.rows.len(), 2);
        assert!(!v.selected_row().unwrap().main_menu);
        v.up();
        assert!(v.selected_row().unwrap().main_menu);
    }

    #[test]
    fn a_disallowed_weapon_has_no_row() {
        let v = view(&menu(), &Localized::default(), |c| {
            (c == "scr_allow_m1carbine").then(|| "0".into())
        });
        assert_eq!(v.rows.len(), 1);
        assert_eq!(v.rows[0].response, "m1garand_mp");
    }

    #[test]
    fn selection_starts_on_the_first_keyed_row() {
        let m = vcod_common::menu::parse(
            r#"{ menuDef { name "team_x"
      itemDef { name "v" visible 1 text "View Map" action { scriptMenuResponse "viewmap"; } }
      itemDef { name "a" visible 1 text "Allies" action { scriptMenuResponse "allies"; } }
      execKey "1" { scriptMenuResponse "allies"; }
    } }"#,
        );
        let v = view(&m, &Localized::default(), |_| None);
        assert_eq!(
            v.selected_row().map(|r| r.response.as_str()),
            Some("allies")
        );

        let unkeyed = vcod_common::menu::parse(
            r#"{ menuDef { name "x"
      itemDef { name "v" visible 1 text "View Map" action { scriptMenuResponse "viewmap"; } }
    } }"#,
        );
        assert_eq!(view(&unkeyed, &Localized::default(), |_| None).selected, 0);
    }

    #[test]
    fn a_menu_with_no_file_closes_in_join_too() {
        let mut cs = vec![String::new(); 1300];
        cs[1180] = "team_missing".into();
        let mut join = Join::new(None, None);
        join.on_server_command(&["t".into(), "0".into()], &cs, 3);
        assert!(join.open().is_some());
        let mut drawn = None;
        sync(
            &mut drawn,
            &mut join,
            &mut MenuCache::default(),
            &Pk3Fs::empty(),
            &Localized::default(),
            &cs,
        );
        assert!(drawn.is_none());
        assert!(join.open().is_none());
    }

    #[test]
    fn selection_wraps() {
        let mut v = view(&menu(), &Localized::default(), |_| None);
        v.up();
        assert_eq!(
            v.selected_row().map(|r| r.response.as_str()),
            Some("m1garand_mp")
        );
        v.down();
        assert_eq!(
            v.selected_row().map(|r| r.response.as_str()),
            Some("m1carbine_mp")
        );
    }

    #[test]
    fn the_mouse_selects_the_row_it_enters() {
        let mut v = view(&menu(), &Localized::default(), |_| None);
        let (w, h) = (1280.0, 960.0);
        let [x, y, _, rh] = row_rect(1, w, h);
        let (cx, cy) = (x + 10.0, y + rh / 2.0);
        assert_eq!(v.row_at(cx, cy, w, h), Some(1));
        assert_eq!(v.mouse_move(cx, cy, w, h), Some(1));
        assert_eq!(v.selected, 1);
        // Moving inside the row selects nothing again.
        assert_eq!(v.mouse_move(cx + 5.0, cy, w, h), None);
        // Off every row the selection stays for the keys.
        assert_eq!(v.mouse_move(0.0, 0.0, w, h), None);
        assert_eq!(v.hover, None);
        assert_eq!(v.selected, 1);
        // An arrow moved the selection away; entering the row takes it back.
        v.up();
        assert_eq!(v.mouse_move(cx, cy, w, h), Some(1));
        let [_, y0, _, _] = row_rect(0, w, h);
        assert_eq!(v.row_at(cx, y0 - 1.0, w, h), None, "above the first row");
    }

    #[test]
    fn the_quick_message_menu_comes_from_ingame_txt_with_a_row_per_exec_key() {
        let mut fs = Pk3Fs::empty();
        fs.overlay(
            "ui_mp/ingame.txt",
            br#"{ loadMenu { "ui_mp/ingame.menu" "ui_mp/wm_quickmessage.menu" } }"#.to_vec(),
        );
        fs.overlay(
            "ui_mp/wm_quickmessage.menu",
            br#"{ menuDef { name "quickmessage"
      itemDef { name "window" visible 1 text "@QUICKMESSAGE_QUICK_MESSAGE" decoration }
      itemDef { name "window" visible 1 text "1. Commands" decoration }
      execKey "1" { close quickmessage; open quickcommands }
      itemDef { name "window" visible 1 text "Esc. Exit" decoration }
    } }"#
                .to_vec(),
        );
        fs.overlay(
            "ui_mp/german/wm_quickmessage.menu",
            br#"{ menuDef { name "quickmessage" } }"#.to_vec(),
        );
        let mut join = Join::new(None, None);
        join.open_quick_message();
        let mut drawn = None;
        let mut menus = MenuCache::new("0");
        sync(
            &mut drawn,
            &mut join,
            &mut menus,
            &fs,
            &Localized::default(),
            &[],
        );
        let (name, v) = drawn.unwrap();
        assert_eq!(name, "quickmessage");
        assert_eq!(v.rows.len(), 1);
        let row = &v.rows[v.row_for_key("1").unwrap()];
        assert_eq!(row.label, "1. Commands");
        assert_eq!(row.opens.as_deref(), Some("quickcommands"));
        // `cl_language` 2 reads the German file first.
        let v = view(
            MenuCache::new("2").get_ingame(&fs, "quickmessage").unwrap(),
            &Localized::default(),
            |_| None,
        );
        assert!(v.rows.is_empty());
    }
}
