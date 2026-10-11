//! The stock team/weapon menu handshake as a pure state machine
//! (docs/research/cod11-hud-protocol.md, section 0.1). `JoinProbe`
//! (`crates/client/src/probe.rs`) is the headless reference this mirrors,
//! minus the socket.

use super::cvars::ClientCvars;
use vcod_common::net::protocol::CsRange;

/// Configstrings 1180..=1211 name the 32 script menus a `t <index>` opens.
const SCRIPT_MENUS: std::ops::RangeInclusive<usize> = {
    let (first, last) = CsRange::Menu.bounds();
    first..=last
};

/// The menu currently open, waiting on a `choose` or a `close`: a script
/// menu by its configstring index, or with no index the UI's `quickmessage`
/// (`mp_QuickMessage`), which sends no response of its own.
pub struct OpenMenu {
    pub index: Option<i32>,
    pub name: String,
}

/// The UI menu `mp_QuickMessage` opens (`UI_SetActiveMenu` case 8,
/// docs/research/cod11-quick-chat.md, "The quick message menu").
pub const QUICK_MESSAGE_MENU: &str = "quickmessage";

/// Drives the handshake for one connection: which menu is open, and which
/// indices an auto-answer has already been sent for since the last
/// `on_gamestate`/`on_restart`.
pub struct Join {
    team: Option<String>,
    weapon: Option<String>,
    open: Option<OpenMenu>,
    /// Menu indices an auto-answer already went out for, so a menu the
    /// server refuses and reopens is offered to the user instead of
    /// answered again (`JoinProbe::answered`).
    auto_answered: Vec<i32>,
    /// The UI key catcher `mp_QuickMessage` set with no menu to show; the
    /// next key goes to the UI and is lost ([`Join::take_stray_key`]).
    stray_catcher: bool,
    pub cvars: ClientCvars,
}

/// `team_`/`weapon_` menu names auto-answer with the matching field, when
/// set; anything else has no scripted reply.
fn auto_reply<'a>(name: &str, team: Option<&'a str>, weapon: Option<&'a str>) -> Option<&'a str> {
    if name.starts_with("team_") {
        return team;
    }
    if name.starts_with("weapon_") {
        return weapon;
    }
    None
}

impl Join {
    pub fn new(team: Option<String>, weapon: Option<String>) -> Self {
        Self {
            team,
            weapon,
            open: None,
            auto_answered: Vec::new(),
            stray_catcher: false,
            cvars: ClientCvars::default(),
        }
    }

    /// `t <idx>` opens the script menu `configstrings[1180 + idx]` names,
    /// answered at once if it is a `team_`/`weapon_` menu covered by
    /// `Join::new`'s arguments and not already auto-answered since the last
    /// rearm; `u` closes whatever is open. Everything else, including `v`,
    /// which `self.cvars` already consumed, is ignored. Returns the
    /// reliable `mr` commands to send.
    pub fn on_server_command(
        &mut self,
        tokens: &[String],
        configstrings: &[String],
        server_id: i32,
    ) -> Vec<String> {
        self.cvars.on_server_command(tokens);
        match tokens.first().map(String::as_str) {
            Some("t") => {
                let Some(idx) = tokens.get(1).and_then(|t| t.parse::<i32>().ok()) else {
                    return Vec::new();
                };
                let Some(name) = usize::try_from(idx)
                    .ok()
                    .map(|i| SCRIPT_MENUS.start() + i)
                    .filter(|cs| SCRIPT_MENUS.contains(cs))
                    .and_then(|cs| configstrings.get(cs))
                    .filter(|n| !n.is_empty())
                else {
                    log::warn!("t {idx}: no script menu at that configstring index");
                    return Vec::new();
                };
                if !self.auto_answered.contains(&idx)
                    && let Some(reply) =
                        auto_reply(name, self.team.as_deref(), self.weapon.as_deref())
                {
                    self.auto_answered.push(idx);
                    self.open = None;
                    return vec![format!("mr {server_id} {idx} {reply}")];
                }
                self.stray_catcher = false;
                self.open = Some(OpenMenu {
                    index: Some(idx),
                    name: name.clone(),
                });
                Vec::new()
            }
            Some("u") => {
                self.close();
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    pub fn open(&self) -> Option<&OpenMenu> {
        self.open.as_ref()
    }

    /// The `mr` for the open menu, closing it. `None` if nothing is open
    /// or the open menu is not a script menu.
    pub fn choose(&mut self, response: &str, server_id: i32) -> Option<String> {
        let index = self.open.take()?.index?;
        Some(format!("mr {server_id} {index} {response}"))
    }

    /// Esc. The stock gametypes ignore the `close` response (`dm.gsc:242`),
    /// so no command goes out.
    pub fn close(&mut self) {
        self.open = None;
    }

    /// The M key: opens `g_scriptMainMenu`'s menu.
    pub fn open_main(&mut self, configstrings: &[String]) {
        if let Some(name) = self.cvars.get("g_scriptMainMenu", configstrings) {
            self.open_script(&name, configstrings);
        }
    }

    /// `open <name>` from a menu: the script menu of that name, looked up in
    /// the script-menu configstring range as `scriptMenuResponse` looks it
    /// up (ui_mp_x86.dll 0x40011140). A menu the server never precached
    /// opens nothing.
    pub fn open_script(&mut self, name: &str, configstrings: &[String]) {
        let Some(index) = SCRIPT_MENUS
            .clone()
            .find(|&i| configstrings.get(i).is_some_and(|n| n == name))
        else {
            return;
        };
        self.open = Some(OpenMenu {
            index: Some((index - SCRIPT_MENUS.start()) as i32),
            name: name.to_string(),
        });
    }

    /// `mp_QuickMessage`: the UI's quick message menu, in place of whatever
    /// menu is open (`Menus_CloseAll` first).
    pub fn open_quick_message(&mut self) {
        self.stray_catcher = false;
        self.open = Some(OpenMenu {
            index: None,
            name: QUICK_MESSAGE_MENU.to_string(),
        });
    }

    /// `mp_QuickMessage` with no `quickmessage` loaded (after `ui_load`):
    /// `UI_SetActiveMenu` case 8 closes every menu and still sets the UI key
    /// catcher (docs/research/cod11-quick-chat.md).
    pub fn quick_message_missing(&mut self) {
        self.open = None;
        self.stray_catcher = true;
    }

    /// A key press while the stray catcher is up: `_UI_KeyEvent` finds no
    /// menu, clears the catcher and drops the key (ui_mp_x86.dll
    /// 0x4000d752). True when this key is the one lost.
    pub fn take_stray_key(&mut self) -> bool {
        std::mem::take(&mut self.stray_catcher)
    }

    /// The stray catcher holds the mouse too (`CL_MouseEvent`, CoDMP.exe
    /// 0x40b0a0): the view does not turn until the next key.
    pub fn stray_catcher(&self) -> bool {
        self.stray_catcher
    }

    /// A new gamestate reruns retail's `ClientConnect`, which puts the
    /// client back on the menus (`JoinProbe::rearm`).
    pub fn on_gamestate(&mut self) {
        self.rearm();
    }

    /// `n`: under `dm`, a round restart reopens the team menu under the
    /// index it last used; under `sd`, whose `pers[]` survives, nothing
    /// reopens and this is inert (`JoinProbe::reopen_menus`).
    pub fn on_restart(&mut self) {
        self.rearm();
    }

    fn rearm(&mut self) {
        self.open = None;
        self.auto_answered.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cs() -> Vec<String> {
        let mut cs = vec![String::new(); 1300];
        cs[1180] = "team_russiangerman".into();
        cs[1181] = "weapon_russian".into();
        cs
    }
    fn toks(s: &str) -> Vec<String> {
        s.split(' ').map(str::to_string).collect()
    }

    #[test]
    fn t_opens_the_menu_its_configstring_names() {
        let mut j = Join::new(None, None);
        assert!(
            j.on_server_command(&toks("v g_scriptMainMenu team_russiangerman"), &cs(), 7)
                .is_empty()
        );
        assert!(j.on_server_command(&toks("t 0"), &cs(), 7).is_empty());
        let open = j.open().unwrap();
        assert_eq!(
            (open.index, open.name.as_str()),
            (Some(0), "team_russiangerman")
        );
        assert_eq!(j.choose("allies", 7).as_deref(), Some("mr 7 0 allies"));
        assert!(j.open().is_none());
    }

    #[test]
    fn a_missing_quick_message_menu_eats_the_next_key_only() {
        let mut j = Join::new(None, None);
        j.on_server_command(&toks("t 0"), &cs(), 7);
        j.quick_message_missing();
        assert!(j.open().is_none());
        assert!(j.stray_catcher());
        assert!(j.take_stray_key());
        assert!(!j.take_stray_key());
        // A script menu that opens first takes the catcher over.
        j.quick_message_missing();
        j.on_server_command(&toks("t 0"), &cs(), 7);
        assert!(!j.take_stray_key());
    }

    #[test]
    fn u_closes() {
        let mut j = Join::new(None, None);
        j.on_server_command(&toks("t 0"), &cs(), 7);
        j.on_server_command(&toks("u"), &cs(), 7);
        assert!(j.open().is_none());
    }

    #[test]
    fn auto_answers_team_then_weapon() {
        let mut j = Join::new(Some("axis".into()), Some("kar98k_mp".into()));
        assert_eq!(
            j.on_server_command(&toks("t 0"), &cs(), 3),
            vec!["mr 3 0 axis"]
        );
        assert_eq!(
            j.on_server_command(&toks("t 1"), &cs(), 3),
            vec!["mr 3 1 kar98k_mp"]
        );
        assert!(j.open().is_none());
    }

    #[test]
    fn auto_weapon_answers_once_per_open() {
        let mut j = Join::new(None, Some("bogus_mp".into()));
        assert_eq!(j.on_server_command(&toks("t 1"), &cs(), 3).len(), 1);
        // The server refused and reopened the same menu: now the user picks.
        assert!(j.on_server_command(&toks("t 1"), &cs(), 3).is_empty());
        assert_eq!(j.open().unwrap().name, "weapon_russian");
    }

    #[test]
    fn rearm_answers_again() {
        let mut j = Join::new(Some("allies".into()), None);
        assert_eq!(j.on_server_command(&toks("t 0"), &cs(), 3).len(), 1);
        j.on_restart();
        assert_eq!(
            j.on_server_command(&toks("t 0"), &cs(), 4),
            vec!["mr 4 0 allies"]
        );
        j.on_gamestate();
        assert_eq!(
            j.on_server_command(&toks("t 0"), &cs(), 5),
            vec!["mr 5 0 allies"]
        );
    }

    #[test]
    fn a_t_naming_an_empty_slot_opens_nothing() {
        let mut j = Join::new(None, None);
        assert!(j.on_server_command(&toks("t 9"), &cs(), 3).is_empty());
        assert!(j.open().is_none());
    }

    /// Index 32 is past the 32 script-menu slots: cs 1212 is the first hint
    /// string, not a menu.
    #[test]
    fn a_t_past_the_script_menu_range_opens_nothing() {
        let mut cs = cs();
        cs[1212] = "team_hintstring".into();
        let mut j = Join::new(Some("allies".into()), None);
        assert!(j.on_server_command(&toks("t 32"), &cs, 3).is_empty());
        assert!(j.open().is_none());
    }

    #[test]
    fn m_opens_the_main_menu() {
        let mut j = Join::new(None, None);
        j.on_server_command(&toks("v g_scriptMainMenu weapon_russian"), &cs(), 3);
        j.open_main(&cs());
        assert_eq!(j.open().unwrap().index, Some(1));
    }

    #[test]
    fn the_quick_message_menu_answers_through_the_script_menu_it_opens() {
        let mut cs = cs();
        cs[1185] = "quickcommands".into();
        let mut j = Join::new(None, None);
        j.on_server_command(&toks("t 0"), &cs, 3);
        j.open_quick_message();
        let open = j.open().unwrap();
        assert_eq!((open.index, open.name.as_str()), (None, QUICK_MESSAGE_MENU));
        // The quick message menu itself sends nothing.
        assert_eq!(j.choose("1", 3), None);
        j.open_quick_message();
        j.open_script("quickcommands", &cs);
        assert_eq!(j.choose("3", 3).as_deref(), Some("mr 3 5 3"));
        j.open_script("quickstatements", &cs);
        assert!(j.open().is_none());
    }
}
