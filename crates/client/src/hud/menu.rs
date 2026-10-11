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
pub struct MenuRow {
    pub label: String,
    pub response: String,
    pub key: Option<String>,
    pub main_menu: bool,
    pub opens: Option<String>,
}

/// A menu's live state: the rows a client can pick from and which is
/// highlighted.
pub struct MenuView {
    pub title: String,
    pub background: Option<String>,
    pub rows: Vec<MenuRow>,
    pub selected: usize,
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
            let key = menu
                .exec_keys
                .iter()
                .find(|k| k.response.as_ref() == Some(&response))
                .map(|k| k.key.clone());
            MenuRow {
                label: loc.translate(&item.text).into_owned(),
                response,
                key,
                main_menu: item.opens_main(),
                opens: None,
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
            })
            .collect();
    }
    let selected = rows.iter().position(|r| r.key.is_some()).unwrap_or(0);
    MenuView {
        title: menu.name.clone(),
        background: menu.background.clone(),
        rows,
        selected,
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

    pub fn row_for_key(&self, key: &str) -> Option<&MenuRow> {
        self.rows.iter().find(|r| r.key.as_deref() == Some(key))
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
    let panel_scale = screen_h / REF_H;
    let w = PANEL_W * panel_scale;
    let h = PANEL_H * panel_scale;
    let x = (screen_w - w) / 2.0;
    let y = (screen_h - h) / 2.0;

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
        let ry = y + (ROWS_TOP + ROW_GAP * i as f32) * panel_scale;
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
      itemDef { name "a" visible 1 text "@MPMENU_1_M1A1_CARBINE" cvartest "scr_allow_m1carbine" hideCvar { "0" } action { scriptMenuResponse "m1carbine_mp"; } }
      execKey "1" { scriptMenuResponse "m1carbine_mp"; }
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
            v.row_for_key("1").map(|r| r.response.as_str()),
            Some("m1carbine_mp")
        );
        assert_eq!(v.row_for_key("2").map(|r| r.response.as_str()), None);
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
        let row = v.row_for_key("1").unwrap();
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
