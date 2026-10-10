//! Start New Server (docs/research/cod11-front-end.md, section 18): the map
//! and game type lists `ui_mp/createserver.menu` shows, and the `vcod-server`
//! child process its Start button launches in place of retail's in-process
//! `map` command.

use std::fs::File;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use anyhow::{Context, Result};
use vcod_common::localize::Localized;
use vcod_common::pk3::Pk3Fs;
use vcod_common::ui_menu::{UiItem, script_commands};

use super::{Painter, SCROLLBAR_SIZE, Ui, UiEffect, truncate_visible, visible_rows};
use crate::hud::HudQuad;

/// One `mp/*.arena` entry (`UI_LoadArenas`, ui_mp_x86.dll 0x40007400).
#[derive(Debug, Clone, PartialEq)]
pub struct Arena {
    /// `map`: the bsp name `map` loads.
    pub map: String,
    /// `longname`: the list's text.
    pub long_name: String,
    /// `gametype`'s words; `None` when the key is missing or empty, which
    /// retail reads as every game type.
    pub gametypes: Option<Vec<String>>,
}

impl Arena {
    pub fn allows(&self, gametype: &str) -> bool {
        self.gametypes
            .as_ref()
            .is_none_or(|g| g.iter().any(|t| t.eq_ignore_ascii_case(gametype)))
    }
}

/// `UI_LoadInfosFromFile`'s caps: 64 infos in all (0x40007170), files of up
/// to 8191 bytes (0x400072f0).
const MAX_ARENAS: usize = 64;
const MAX_ARENA_FILE: usize = 0x1fff;

/// Every `{ key value ... }` block of an info file, as key/value pairs.
fn info_blocks(text: &str) -> Vec<Vec<(String, String)>> {
    let tokens = tokens(text);
    let mut out = Vec::new();
    let mut it = tokens.into_iter();
    while let Some(t) = it.next() {
        if t != "{" {
            // "Missing { in info file".
            break;
        }
        let mut pairs = Vec::new();
        while let Some(key) = it.next() {
            if key == "}" {
                break;
            }
            let value = it.next().unwrap_or_default();
            pairs.push((key.to_ascii_lowercase(), value));
        }
        out.push(pairs);
    }
    out
}

/// `COM_Parse`'s words: whitespace-split, quotes kept together, `//`
/// comments skipped.
fn tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '/' && text_starts_comment(&mut chars) {
            for c in chars.by_ref() {
                if c == '\n' {
                    break;
                }
            }
        } else if c == '"' {
            chars.next();
            out.push(chars.by_ref().take_while(|&c| c != '"').collect());
        } else {
            let mut w = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                w.push(c);
                chars.next();
            }
            out.push(w);
        }
    }
    out
}

fn text_starts_comment(chars: &mut std::iter::Peekable<std::str::Chars>) -> bool {
    let mut ahead = chars.clone();
    ahead.next();
    ahead.peek() == Some(&'/')
}

/// `UI_LoadArenas`: the `.arena` files directly under `mp/`, in name order.
pub fn load_arenas(fs: &Pk3Fs) -> Vec<Arena> {
    let mut out = Vec::new();
    for path in fs.list_prefix("mp/") {
        let name = &path["mp/".len()..];
        if name.contains('/') || !name.ends_with(".arena") {
            continue;
        }
        let Some(bytes) = fs.read(&path) else {
            continue;
        };
        if bytes.len() > MAX_ARENA_FILE {
            log::warn!("{path} is {} bytes, past {MAX_ARENA_FILE}", bytes.len());
            continue;
        }
        for info in info_blocks(&String::from_utf8_lossy(&bytes)) {
            if out.len() == MAX_ARENAS {
                log::warn!("Max infos exceeded");
                return out;
            }
            let get = |k: &str| {
                info.iter()
                    .find(|(key, _)| key == k)
                    .map_or(String::new(), |(_, v)| v.clone())
            };
            let gametype = get("gametype");
            out.push(Arena {
                map: get("map"),
                long_name: get("longname"),
                gametypes: (!gametype.is_empty())
                    .then(|| gametype.split_whitespace().map(str::to_string).collect()),
            });
        }
    }
    out
}

/// What `uiScript StartServer` (0x4000a62b) asks for: retail sets
/// `dedicated` to `ui_dedicated` clamped to 0..2 and `g_gametype`, then runs
/// `map`; vcod hands the same to a `vcod-server` it starts.
#[derive(Debug, Clone, PartialEq)]
pub struct StartServer {
    pub map: String,
    pub gametype: String,
    pub dedicated: i32,
    /// The cvars the create server and game type settings menus edit; the
    /// app reads their values when it launches.
    pub cvars: Vec<String>,
}

/// The `vcod-server` command line for `req`, with `value` reading a cvar
/// (`None` or empty: left at the server's default). `sv_hostname` and
/// `sv_maxclients` go through their own flags, every other cvar through
/// `--set`.
pub fn server_args(
    req: &StartServer,
    port: u16,
    game_dir: &Path,
    mod_dir: &str,
    value: &dyn Fn(&str) -> Option<String>,
) -> Vec<String> {
    let mut args = vec![
        req.map.clone(),
        "--game-dir".into(),
        game_dir.display().to_string(),
        "--mod-dir".into(),
        mod_dir.into(),
        "--port".into(),
        port.to_string(),
        "--gametype".into(),
        req.gametype.clone(),
        "--set".into(),
        format!("dedicated={}", req.dedicated),
    ];
    for name in &req.cvars {
        let Some(v) = value(name).filter(|v| !v.is_empty()) else {
            continue;
        };
        if name.eq_ignore_ascii_case("sv_hostname") {
            args.extend(["--hostname".into(), v]);
        } else if name.eq_ignore_ascii_case("sv_maxclients") {
            // The server takes a count; a field left at junk keeps its default.
            if let Ok(n) = v.trim().parse::<usize>()
                && n > 0
            {
                args.extend(["--max-clients".into(), n.to_string()]);
            }
        } else {
            args.extend(["--set".into(), format!("{name}={v}")]);
        }
    }
    args
}

impl Ui {
    /// `ui_netGametype` as an index into the game type list; one past the
    /// end reads as 0 (`UI_DrawNetGameType`, 0x40008820).
    fn net_gametype(&self) -> usize {
        let i = self
            .ui_cvar("ui_netGametype")
            .trim()
            .parse::<usize>()
            .unwrap_or(0);
        if i < self.game_types.len() { i } else { 0 }
    }

    /// The maps the selected game type allows, in load order
    /// (0x4000b5c0 marks them).
    pub(super) fn active_maps(&self) -> Vec<&Arena> {
        let gt = self
            .game_types
            .get(self.net_gametype())
            .map_or("", |g| g.script.as_str());
        self.arenas.iter().filter(|a| a.allows(gt)).collect()
    }

    fn select_gametype(&mut self, i: usize, out: &mut Vec<UiEffect>) {
        let name = self
            .game_types
            .get(i)
            .map_or(String::new(), |g| g.script.clone());
        self.set_cvar("ui_netGametype", &i.to_string(), out);
        self.set_cvar("ui_netGametypeName", &name, out);
        self.map_row = 0;
        self.map_top = 0;
    }

    /// `uiScript loadArenas` (0x4000a980): the game type follows
    /// `g_gametype` when the list has it (0x4000b510), and the first map
    /// is selected.
    pub(super) fn load_arenas(&mut self, out: &mut Vec<UiEffect>) {
        let current = self.ui_cvar("g_gametype");
        match self
            .game_types
            .iter()
            .position(|g| g.script.eq_ignore_ascii_case(&current))
        {
            Some(i) => self.select_gametype(i, out),
            None => {
                self.map_row = 0;
                self.map_top = 0;
            }
        }
    }

    /// A click on `UI_NETGAMETYPE` (0x40009a60): the next game type,
    /// wrapping, and the map list starts over.
    pub(super) fn step_gametype(&mut self, out: &mut Vec<UiEffect>) {
        if self.game_types.is_empty() {
            return;
        }
        let next = (self.net_gametype() + 1) % self.game_types.len();
        self.select_gametype(next, out);
        self.set_cvar("ui_currentNetMap", "0", out);
    }

    /// `UI_STARTMAPCINEMATIC`'s image: the selected map's levelshot, which
    /// `UI_LoadArenas` names `levelshots/<map>`.
    pub(super) fn start_map_preview(&self) -> String {
        let map = self.active_maps().get(self.map_row).map(|a| a.map.clone());
        self.levelshot(&map.unwrap_or_default())
    }

    /// `UI_NETGAMETYPE`'s text: the game type's localized name, `EXE_ALL`
    /// when it has none.
    pub(super) fn gametype_text(&self, loc: &Localized) -> String {
        let key = self
            .game_types
            .get(self.net_gametype())
            .map_or("EXE_ALL", |g| g.name.as_str());
        loc.get(key).unwrap_or(key).to_string()
    }

    /// `uiScript StartServer` (0x4000a62b): the selected map with the game
    /// type and `ui_dedicated` clamped to 0..2. `g_gametype` takes the
    /// game type, as retail sets it.
    pub(super) fn start_server(&mut self, out: &mut Vec<UiEffect>) {
        let Some(map) = self.active_maps().get(self.map_row).map(|a| a.map.clone()) else {
            crate::console::log::print("No map selected.");
            return;
        };
        let Some(gametype) = self
            .game_types
            .get(self.net_gametype())
            .map(|g| g.script.clone())
        else {
            return;
        };
        let dedicated = self
            .ui_cvar("ui_dedicated")
            .trim()
            .parse::<f32>()
            .unwrap_or(0.0)
            .clamp(0.0, 2.0) as i32;
        self.set_cvar("g_gametype", &gametype, out);
        out.push(UiEffect::StartServer(StartServer {
            map,
            gametype,
            dedicated,
            cvars: self.server_cvars(),
        }));
    }

    /// The cvars `createserver_op` and the `settings_*` menus edit, their
    /// `ui_` shadows left out: what a listen server would read off the
    /// shared console.
    fn server_cvars(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        let menus = self.menus.iter().filter(|m| {
            let n = m.name.to_ascii_lowercase();
            n == "createserver_op" || n.starts_with("settings_")
        });
        for item in menus.flat_map(|m| &m.items) {
            let set = script_commands(&item.action)
                .into_iter()
                .filter(|c| c.len() > 1 && c[0].eq_ignore_ascii_case("setcvar"))
                .map(|c| c[1].clone());
            for name in item.cvar.clone().into_iter().chain(set) {
                if !name.to_ascii_lowercase().starts_with("ui_")
                    && !names.iter().any(|n| n.eq_ignore_ascii_case(&name))
                {
                    names.push(name);
                }
            }
        }
        names
    }

    /// A click on `FEEDER_ALLMAPS`: selects the row under the cursor.
    pub(super) fn map_click(&mut self, item: &UiItem) {
        let row = ((self.cursor[1] - item.rect[1] - 1.0) / item.element_height.max(1.0)).floor();
        let row = row.max(0.0) as usize + self.map_top;
        if row < self.active_maps().len() {
            self.map_row = row;
        }
    }

    /// The wheel over the map list; false when the cursor is elsewhere.
    pub(super) fn scroll_maps(&mut self, up: bool) -> bool {
        let Some((m, i)) = self.hover else {
            return false;
        };
        let item = &self.menus[m].items[i];
        if item.feeder != Some(vcod_common::ui_menu::FEEDER_ALLMAPS) {
            return false;
        }
        let n = visible_rows(item);
        let len = self.active_maps().len();
        self.map_top = if up {
            self.map_top.saturating_sub(1)
        } else {
            (self.map_top + 1).min(len.saturating_sub(n))
        };
        true
    }

    /// `Item_ListBox_Paint` for `FEEDER_ALLMAPS`: each allowed map's
    /// `longname` (0x4000c6c0).
    pub(super) fn map_rows(&self, p: &Painter, item: &UiItem, out: &mut Vec<HudQuad>) {
        let [x, y0, w, _] = item.rect;
        let (x, mut y) = (x + 1.0, y0 + 1.0);
        let eh = item.element_height.max(1.0);
        let col = item.columns.first().copied();
        let maps = self.active_maps();
        for (row, a) in maps
            .iter()
            .enumerate()
            .skip(self.map_top)
            .take(visible_rows(item))
        {
            let text = truncate_visible(&a.long_name, col.map_or(0, |c| c.max_chars));
            p.text(
                &text,
                x + 4.0 + col.map_or(0.0, |c| c.pos) + item.text_align_x,
                y + eh + item.text_align_y,
                item.text_scale,
                item.fore,
                out,
            );
            if row == self.map_row {
                p.fill(
                    [x, y, w - SCROLLBAR_SIZE - 4.0, eh - 1.0],
                    item.outline_color,
                    out,
                );
            }
            y += eh;
        }
    }
}

/// The first UDP port from `start` that binds, for the child to take.
pub fn free_port(start: u16) -> Option<u16> {
    (start..start.saturating_add(32)).find(|&p| UdpSocket::bind(("0.0.0.0", p)).is_ok())
}

/// A `vcod-server` this client started. Dropping it kills the process.
pub struct ListenServer {
    child: Child,
    pub port: u16,
    pub log: PathBuf,
}

impl ListenServer {
    /// Starts `exe` with `args`, its output going to `log` (truncated).
    pub fn spawn(exe: &Path, args: &[String], port: u16, log: PathBuf) -> Result<ListenServer> {
        let out = File::create(&log).with_context(|| format!("creating {}", log.display()))?;
        let err = out.try_clone()?;
        let child = Command::new(exe)
            .args(args)
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err)
            .spawn()
            .with_context(|| format!("starting {}", exe.display()))?;
        Ok(ListenServer { child, port, log })
    }

    /// The exit status once the process has ended on its own.
    pub fn exited(&mut self) -> Option<std::process::ExitStatus> {
        self.child.try_wait().ok().flatten()
    }
}

impl Drop for ListenServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `vcod-server` beside the running executable.
pub fn server_exe() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("locating vcod")?;
    let dir = exe.parent().context("vcod has no directory")?;
    let path = dir.join(format!("vcod-server{}", std::env::consts::EXE_SUFFIX));
    anyhow::ensure!(
        path.is_file(),
        "vcod-server is not next to vcod ({})",
        path.display()
    );
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_blocks_parse_and_an_empty_gametype_allows_all() {
        let infos = info_blocks(
            "{\n\tmap \"mp_a\"\n\tlongname \"A\" // note\n\tgametype \"dm tdm\"\n}\n{ map mp_b longname B }",
        );
        assert_eq!(infos.len(), 2);
        assert_eq!(infos[0][2], ("gametype".into(), "dm tdm".into()));
        let a = Arena {
            map: "mp_a".into(),
            long_name: "A".into(),
            gametypes: Some(vec!["dm".into(), "TDM".into()]),
        };
        assert!(a.allows("tdm") && !a.allows("sd"));
        let b = Arena {
            gametypes: None,
            ..a
        };
        assert!(b.allows("sd"));
    }

    #[test]
    fn start_maps_the_menu_cvars_onto_the_command_line() {
        let req = StartServer {
            map: "mp_carentan".into(),
            gametype: "tdm".into(),
            dedicated: 2,
            cvars: [
                "sv_hostname",
                "sv_maxclients",
                "sv_pure",
                "g_password",
                "scr_tdm_timelimit",
            ]
            .map(String::from)
            .to_vec(),
        };
        let value = |n: &str| match n {
            "sv_hostname" => Some("My Host".to_string()),
            "sv_maxclients" => Some("12".to_string()),
            "sv_pure" => Some("0".to_string()),
            "g_password" => Some(String::new()),
            _ => None,
        };
        let args = server_args(&req, 28961, Path::new("/cod"), "main", &value);
        assert_eq!(
            args,
            [
                "mp_carentan",
                "--game-dir",
                "/cod",
                "--mod-dir",
                "main",
                "--port",
                "28961",
                "--gametype",
                "tdm",
                "--set",
                "dedicated=2",
                "--hostname",
                "My Host",
                "--max-clients",
                "12",
                "--set",
                "sv_pure=0",
            ]
        );
    }

    #[test]
    fn a_bad_player_count_keeps_the_default() {
        let req = StartServer {
            map: "m".into(),
            gametype: "dm".into(),
            dedicated: 0,
            cvars: vec!["sv_maxclients".into()],
        };
        let args = server_args(&req, 1, Path::new("d"), "main", &|_| Some("x".into()));
        assert!(!args.iter().any(|a| a == "--max-clients"));
    }

    #[test]
    fn stock_arenas_filter_by_game_type() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let arenas = load_arenas(&fs);
        let carentan = arenas.iter().find(|a| a.map == "mp_carentan").unwrap();
        assert!(carentan.allows("bel"));
        let chateau = arenas.iter().find(|a| a.map == "mp_chateau").unwrap();
        assert!(!chateau.allows("sd"));
    }
}
