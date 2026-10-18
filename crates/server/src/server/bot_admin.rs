//! The server half of bot management (`crate::botmgmt` holds the rules):
//! the `addbot`, `removebot` and `listbots` console commands, and the pass
//! that keeps the bot count, the names and the teams where the
//! `vc_bot_*` cvars say. docs/research/bot-objectives.md, "Bot management".

use super::*;
use crate::botmgmt::{self, Action, Member, Settings, Side, cvar};

/// How often the pass reads the cvars and the roster when nothing asked
/// for one sooner.
const PASS_MS: i32 = 500;

pub(super) struct BotAdmin {
    /// The bots asked for by count: `--bots`, `addbot`, `removebot`.
    manual: usize,
    /// Bots `addbot` put on a team by name; balance leaves them there.
    pinned: std::collections::BTreeSet<usize>,
    /// `vc_bot_shoot` as last applied.
    pub(super) shoot: bool,
    /// The name draw's own generator, so naming a bot never moves the
    /// server's `rand()` sequence the seeded tests replay.
    name_rng: u64,
    last_pass: Option<i32>,
    /// A console command changed something; plan on the next tick.
    dirty: bool,
    /// Team menu answers for moved bots, sent with the bots' own replies.
    pub(super) orders: Vec<(usize, String)>,
}

impl BotAdmin {
    pub(super) fn new(manual: usize, shoot: bool, seed: u64) -> Self {
        BotAdmin {
            manual,
            pinned: Default::default(),
            shoot,
            name_rng: seed ^ 0x9e37_79b9_7f4a_7c15,
            last_pass: None,
            dirty: false,
            orders: Vec::new(),
        }
    }
}

impl Server {
    fn bot_settings(&self) -> Settings {
        let on = |name: &str, default: &str| {
            crate::game::builtins::cvar::atoi(&self.live_cvar(name, default)) != 0
        };
        Settings {
            min_players: self.live_int_cvar(cvar::MIN_PLAYERS).max(0) as usize,
            autobalance: on(cvar::AUTOBALANCE, "1"),
            shoot: on(cvar::SHOOT, "1"),
            names: on(cvar::NAMES, "1"),
        }
    }

    /// Whether the running gametype has two sides to balance: everything
    /// but `dm`, and `bel`, whose script deals the allied places itself.
    fn bot_teamed(&self) -> bool {
        !matches!(
            self.live_gametype().to_ascii_lowercase().as_str(),
            "dm" | "bel"
        )
    }

    /// The free public slots plus those bots hold: what bots may fill.
    fn bot_room(&self) -> usize {
        let private = self.private_clients().max(0) as usize;
        let free = (private..self.clients.len())
            .filter(|&i| self.clients[i].is_none() && self.zombies[i].is_none())
            .count();
        free + self.bots.len()
    }

    /// Every connected client as the planner sees it.
    fn bot_roster(&mut self) -> Vec<Member> {
        let mut out = Vec::new();
        for slot in 0..self.clients.len() {
            let Some(c) = self.clients[slot].as_ref() else {
                continue;
            };
            let bot = self.bots.get(&slot);
            // `pers["team"]`, which the team menu sets at once; `sessionteam`
            // waits for the spawn.
            let (team, state, score) = match self.script.as_mut() {
                Some(rt) => (
                    rt.client_pers(slot, "team").unwrap_or_default(),
                    rt.client_field(slot, "sessionstate").unwrap_or_default(),
                    rt.client_field(slot, "score").unwrap_or_default(),
                ),
                None => Default::default(),
            };
            let side = match Side::parse(&team) {
                Some(s @ (Side::Allies | Side::Axis)) => s,
                // Still joining: the side its answer will take.
                _ => match bot.and_then(|b| Side::parse(b.team())) {
                    Some(s) if c.is_bot && !matches!(state.as_str(), "playing" | "dead") => s,
                    _ => Side::None,
                },
            };
            out.push(Member {
                slot,
                bot: c.is_bot && bot.is_some(),
                side,
                score: score.parse().unwrap_or(0),
                // Dead or spectating is out of the fight; an intermission
                // is not a time to move anyone.
                playing: !matches!(state.as_str(), "dead" | "spectator"),
                pinned: self.bot_admin.pinned.contains(&slot),
            });
        }
        out
    }

    /// The bot pass, at the top of `step_bots`: applies `vc_bot_shoot`,
    /// then joins, drops and moves bots toward what the manual count, the
    /// floor and balance ask for. Runs every [`PASS_MS`], or on the next
    /// tick after a console command.
    pub(super) fn manage_bots(&mut self) {
        if self.script.is_none() {
            return;
        }
        let due = self
            .bot_admin
            .last_pass
            .is_none_or(|t| self.sv_time_ms.wrapping_sub(t) >= PASS_MS);
        if !due && !self.bot_admin.dirty {
            return;
        }
        self.bot_admin.last_pass = Some(self.sv_time_ms);
        self.bot_admin.dirty = false;
        let s = self.bot_settings();
        if s.shoot != self.bot_admin.shoot {
            self.bot_admin.shoot = s.shoot;
            for b in self.bots.values_mut() {
                b.set_shoot(s.shoot);
            }
        }
        self.bot_admin
            .pinned
            .retain(|slot| self.bots.contains_key(slot));
        let roster = self.bot_roster();
        // A human counts once he has picked a side; one still in the menus
        // or spectating takes no bot's place.
        let humans = roster
            .iter()
            .filter(|m| !m.bot && m.side != Side::None)
            .count();
        let want = botmgmt::target(
            self.bot_admin.manual,
            s.min_players,
            humans,
            self.bot_room(),
        );
        let teamed = self.bot_teamed();
        for action in botmgmt::plan(&roster, want, teamed, s.autobalance) {
            match action {
                Action::Add(side) => {
                    if self.add_bot(side, s).is_none() {
                        break;
                    }
                }
                Action::Remove(slot) => self.remove_bot(slot),
                Action::Move(slot, side) => self.move_bot(slot, side),
            }
        }
    }

    /// Connects one bot in the first free public slot, asking for `side`
    /// (allies on `Side::None`). The two `rand()` draws are the ones
    /// `--bots` always made, in its order.
    fn add_bot(&mut self, side: Side, s: Settings) -> Option<usize> {
        let private = self.private_clients().max(0) as usize;
        let Some(slot) = self.free_slot_from(private) else {
            log::warn!("no free slot for a bot");
            return None;
        };
        let team = match side {
            Side::Axis => "axis",
            _ => "allies",
        };
        let mut bot = crate::bots::Bot::new(team, self.bot_admin.shoot, self.rand() as u64);
        if self.move_rules() == vcod_common::pmove::MoveRules::Uo {
            bot.menus = &crate::bots::WEAPON_MENUS_UO;
        }
        let name = if s.names {
            let taken: Vec<String> = self.bots.values().map(|b| b.name.clone()).collect();
            let rng = &mut self.bot_admin.name_rng;
            botmgmt::draw_name(&taken, &mut |n| rand_from(rng) as usize % n)
        } else {
            format!("bot{}", slot + 1)
        };
        bot.name = name.clone();
        let userinfo = format!("\\name\\{name}\\snaps\\20\\rate\\25000\\cl_anonymous\\0");
        // A loopback address nothing listens on; every transmit to a bot
        // is skipped, the addr only has to be unique.
        let addr = SocketAddr::from(([127, 0, 0, 1], 65535 - slot as u16));
        let mut client = Client::new(
            addr,
            0x7000 + slot as u16,
            self.rand(),
            userinfo,
            Instant::now(),
        );
        client.is_bot = true;
        // `status` prints it as a bot address, as retail's test clients.
        client.bot_port = Some(slot as u16 + 1);
        self.clients[slot] = Some(client);
        self.bots.insert(slot, bot);
        if let Some(rt) = self.script.as_mut() {
            rt.push_client_event(ClientEvent::Connect {
                slot,
                name: name.clone(),
            });
        }
        log::info!("client {slot} {name:?} connected (bot, team {team})");
        Some(slot)
    }

    /// Drops a bot through `SV_DropClient`, as a kick would.
    fn remove_bot(&mut self, slot: usize) {
        self.bot_admin.pinned.remove(&slot);
        self.drop_client(slot, "EXE_PLAYERKICKED");
        self.bots.remove(&slot);
    }

    /// Answers the team menu for `side` on a bot's behalf: the stock
    /// handlers switch a dead or spectating player and reopen the weapon
    /// menu, which the bot answers again.
    fn move_bot(&mut self, slot: usize, side: Side) {
        let Some(bot) = self.bots.get_mut(&slot) else {
            return;
        };
        bot.set_team(side.response());
        log::info!("bot {slot} {:?} moves to {}", bot.name, side.response());
        let (first, last) = self.proto.cs.bounds(configstrings::CsRange::Menu);
        let menu = (first..last).find(|&i| {
            self.configstrings
                .get(i)
                .is_some_and(|m| m.starts_with("team_") && m != "team_germanonly")
        });
        if let Some(i) = menu {
            let sid = i32::from(self.server_id);
            let reply = format!("mr {sid} {} {}", i - first, side.response());
            self.bot_admin.orders.push((slot, reply));
        }
    }

    /// `addbot [allies|axis|auto] [count]`, the words in either order. A
    /// named side pins the bots there; `auto` (the default) raises the
    /// count the pass deals out by balance.
    pub(super) fn add_bot_command(&mut self, args: &[String]) {
        let mut side = Side::None;
        let mut count = 1usize;
        for a in args {
            if let Some(s) = Side::parse(a) {
                side = s;
            } else if let Ok(n) = a.parse::<usize>() {
                count = n;
            } else {
                self.print("usage: addbot [allies|axis|auto] [count]\n");
                return;
            }
        }
        if self.script.is_none() {
            self.print("addbot: no level is running\n");
            return;
        }
        let room = self.bot_room().saturating_sub(self.bots.len());
        let count = count.min(room);
        if count == 0 {
            self.print("addbot: no free public slot\n");
            return;
        }
        if side == Side::None {
            self.bot_admin.manual = self.bots.len().max(self.bot_admin.manual) + count;
            self.bot_admin.dirty = true;
            self.print(&format!("adding {count} bot(s)\n"));
            return;
        }
        let s = self.bot_settings();
        let mut added = 0;
        for _ in 0..count {
            let Some(slot) = self.add_bot(side, s) else {
                break;
            };
            self.bot_admin.pinned.insert(slot);
            added += 1;
        }
        self.bot_admin.manual = self.bots.len().max(self.bot_admin.manual + added);
        self.print(&format!("added {added} bot(s) to {}\n", side.response()));
    }

    /// `removebot [name|all|count]`: by name (the `[BOT] ` prefix and
    /// colours optional), every bot, or that many, lowest score first.
    pub(super) fn remove_bot_command(&mut self, arg: Option<&str>) {
        let arg = arg.unwrap_or("1").trim();
        let roster = self.bot_roster();
        let mut bots: Vec<&Member> = roster.iter().filter(|m| m.bot).collect();
        bots.sort_by(|a, b| a.score.cmp(&b.score).then(b.slot.cmp(&a.slot)));
        let picked: Vec<usize> = if arg.eq_ignore_ascii_case("all") {
            bots.iter().map(|m| m.slot).collect()
        } else if let Ok(n) = arg.parse::<usize>() {
            bots.iter().take(n).map(|m| m.slot).collect()
        } else {
            let want = crate::game::say::clean_name(arg);
            let found = bots.iter().find(|m| {
                let name = self.bots.get(&m.slot).map_or("", |b| b.name.as_str());
                let clean = crate::game::say::clean_name(name);
                clean.eq_ignore_ascii_case(&want)
                    || clean
                        .strip_prefix(botmgmt::PREFIX)
                        .is_some_and(|n| n.eq_ignore_ascii_case(&want))
            });
            match found {
                Some(m) => vec![m.slot],
                None => {
                    self.print(&format!("removebot: no bot named {arg}\n"));
                    return;
                }
            }
        };
        let names: Vec<String> = picked
            .iter()
            .filter_map(|s| self.bots.get(s).map(|b| b.name.clone()))
            .collect();
        for &slot in &picked {
            self.remove_bot(slot);
        }
        self.bot_admin.manual = self.bot_admin.manual.saturating_sub(picked.len());
        for n in names {
            self.print(&format!("removed {n}\n"));
        }
        let floor = self.bot_settings().min_players;
        if floor > 0 {
            self.print(&format!(
                "{} {floor} tops the bots back up; set it to 0 to keep them out\n",
                cvar::MIN_PLAYERS
            ));
        }
        self.bot_admin.dirty = true;
    }

    /// `listbots`: slot, team, score and name of every bot, then the count
    /// the server keeps and why.
    pub(super) fn list_bots(&mut self) {
        let roster = self.bot_roster();
        let mut text = String::from("num team    score pinned name\n");
        for m in roster.iter().filter(|m| m.bot) {
            let name = self.bots.get(&m.slot).map_or("", |b| b.name.as_str());
            let team = match m.side {
                Side::Allies => "allies",
                Side::Axis => "axis",
                Side::None => "-",
            };
            let pinned = if m.pinned { "yes" } else { "no" };
            text.push_str(&format!(
                "{:3} {team:<7} {:5} {pinned:<6} {name}\n",
                m.slot, m.score
            ));
        }
        let s = self.bot_settings();
        let humans = roster
            .iter()
            .filter(|m| !m.bot && m.side != Side::None)
            .count();
        text.push_str(&format!(
            "{} bot(s), {humans} human player(s); manual {}, {} {}, {} {}, {} {}\n",
            roster.iter().filter(|m| m.bot).count(),
            self.bot_admin.manual,
            cvar::MIN_PLAYERS,
            s.min_players,
            cvar::AUTOBALANCE,
            u8::from(s.autobalance),
            cvar::SHOOT,
            u8::from(s.shoot),
        ));
        self.print(&text);
    }
}
