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

    /// A `v`-set value, else the 140/204 mirror entry under a
    /// case-insensitive name match.
    pub fn get(&self, name: &str, configstrings: &[String]) -> Option<String> {
        if let Some(v) = self.set.get(&name.to_lowercase()) {
            return Some(v.clone());
        }
        let (names, offset) = self.mirror();
        let i = names.into_iter().find(|&i| {
            configstrings
                .get(i)
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })?;
        configstrings.get(i + offset).cloned()
    }

    /// Every value [`Self::get`] can answer, keyed lowercase.
    pub fn all(&self, configstrings: &[String]) -> HashMap<String, String> {
        let (names, offset) = self.mirror();
        let mut out: HashMap<String, String> = names
            .filter_map(|i| {
                let name = configstrings.get(i).filter(|n| !n.is_empty())?;
                let value = configstrings.get(i + offset)?;
                Some((name.to_lowercase(), value.clone()))
            })
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
        cs[167] = "scr_allow_m1carbine".into();
        cs[167 + 64] = "0".into();
        let c = ClientCvars::default();
        assert_eq!(c.get("scr_allow_m1carbine", &cs).as_deref(), Some("0"));
        assert_eq!(c.get("scr_allow_nope", &cs), None);
        let mut c = c;
        c.on_server_command(&["v".into(), "SCR_allow_m1carbine".into(), "1".into()]);
        assert_eq!(c.all(&cs)["scr_allow_m1carbine"], "1");
    }
}
