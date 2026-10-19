//! Client cvars set by the server's `v` command, plus the 140/204
//! configstring mirror those same names may fall back to
//! (docs/research/cod11-hud-protocol.md, section 0; `crates/server/src/cvars.rs`,
//! the "140/204 mirror").

use std::collections::HashMap;

use vcod_common::net::protocol::{CS_LAYOUT_V1, CsLayout};

/// Cvars a `v` command set, keyed lowercase. A `v` wins over the 140/204
/// mirror `get` also checks.
pub struct ClientCvars {
    set: HashMap<String, String>,
    /// Where the mirror sits: configstrings 140..=203 name the cvars and
    /// 204..=267 hold their values on 1.1, United Offensive's 149 and 277
    /// blocks hold 128 each (`CsLayout`).
    layout: CsLayout,
}

impl Default for ClientCvars {
    fn default() -> Self {
        Self {
            set: HashMap::new(),
            layout: CS_LAYOUT_V1,
        }
    }
}

impl ClientCvars {
    /// The connection's configstring layout.
    pub fn set_layout(&mut self, layout: CsLayout) {
        self.layout = layout;
    }

    /// The mirror's name slots and the offset to each one's value.
    fn mirror(&self) -> (std::ops::RangeInclusive<usize>, usize) {
        let (first, last) = self.layout.cvar_names;
        (first..=last, self.layout.cvar_values - first)
    }

    /// `v <name> "<value>"`; returns whether the command was a `v` this
    /// consumed.
    pub fn on_server_command(&mut self, tokens: &[String]) -> bool {
        if tokens.first().map(String::as_str) != Some("v") {
            return false;
        }
        let Some(name) = tokens.get(1) else {
            return false;
        };
        let value = tokens.get(2).cloned().unwrap_or_default();
        self.set.insert(name.to_lowercase(), value);
        true
    }

    /// `CG_MapRestart`'s `trap_Cvar_Set`s (1.1 cgame 0x3002ca60, uo
    /// 0x30039500; events doc 9.8): the objective text empties, the
    /// stance and run state go back to standing and running.
    pub fn on_map_restart(&mut self) {
        for (name, value) in [
            ("cg_thirdperson", "0"),
            ("cl_stance", "0"),
            ("cl_run", "1"),
            ("cg_objectivetext", ""),
        ] {
            self.set.insert(name.into(), value.into());
        }
    }

    /// The mirror's `(name, value)` pairs as the cgame sets them (1.1
    /// 0x3002bc60, UO 0x30038430): in slot order, stopping at the first
    /// empty name, so a name past a hole is never set.
    fn mirrored<'a>(
        &self,
        configstrings: &'a [String],
    ) -> impl Iterator<Item = (&'a str, &'a str)> {
        let (names, offset) = self.mirror();
        names
            .map(move |i| {
                let name = configstrings.get(i).map_or("", String::as_str);
                let value = configstrings.get(i + offset).map_or("", String::as_str);
                (name, value)
            })
            .take_while(|(name, _)| !name.is_empty())
    }

    /// A `v`-set value, else the 140/204 mirror entry under a
    /// case-insensitive name match.
    pub fn get(&self, name: &str, configstrings: &[String]) -> Option<String> {
        if let Some(v) = self.set.get(&name.to_lowercase()) {
            return Some(v.clone());
        }
        self.mirrored(configstrings)
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.to_owned())
    }

    /// Every value [`Self::get`] can answer, keyed lowercase.
    pub fn all(&self, configstrings: &[String]) -> HashMap<String, String> {
        let mut out: HashMap<String, String> = self
            .mirrored(configstrings)
            .map(|(n, v)| (n.to_lowercase(), v.to_owned()))
            .collect();
        out.extend(self.set.iter().map(|(k, v)| (k.clone(), v.clone())));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v_sets_a_client_cvar() {
        let mut c = ClientCvars::default();
        assert!(c.on_server_command(&["v".into(), "scr_showweapontab".into(), "1".into()]));
        assert!(!c.on_server_command(&["t".into(), "0".into()]));
        assert_eq!(c.get("SCR_SHOWWEAPONTAB", &[]).as_deref(), Some("1"));
    }

    #[test]
    fn mirror_lookup() {
        let mut cs = vec![String::new(); 300];
        for (i, name) in cs[140..167].iter_mut().enumerate() {
            *name = format!("scr_pad{i}");
        }
        cs[167] = "scr_allow_m1carbine".into();
        cs[167 + 64] = "0".into();
        let c = ClientCvars::default();
        assert_eq!(c.get("scr_allow_m1carbine", &cs).as_deref(), Some("0"));
        assert_eq!(c.get("scr_allow_nope", &cs), None);
        let mut c = c;
        c.on_server_command(&["v".into(), "SCR_allow_m1carbine".into(), "1".into()]);
        assert_eq!(c.all(&cs)["scr_allow_m1carbine"], "1");
    }

    /// The cgame stops at the first empty name: a name past a hole is
    /// never mirrored.
    #[test]
    fn mirror_stops_at_the_first_empty_name() {
        let mut cs = vec![String::new(); 300];
        cs[140] = "scr_first".into();
        cs[204] = "1".into();
        cs[142] = "scr_past_hole".into();
        cs[206] = "2".into();
        let c = ClientCvars::default();
        assert_eq!(c.get("scr_first", &cs).as_deref(), Some("1"));
        assert_eq!(c.get("scr_past_hole", &cs), None);
        let all = c.all(&cs);
        assert_eq!(all.len(), 1, "{all:?}");
    }
}
