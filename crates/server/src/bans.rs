//! The ban list. Retail 1.1 keeps none: `banUser` and `banClient` send
//! `banUser <ip>` to the authorize server, which denies that address at its
//! next `getIpAuthorize` (docs/research/cod11-server-handshake.md,
//! "Bans"). 1.5 keeps `ban.txt`, a guid and a cleaned name a line, and
//! `unbanUser` drops lines by name ("1.5 permanent bans"). vcod has no
//! authorize server and so no guid: it keys the list on the address, keeps
//! the name beside it, and answers the challenge the way retail refuses.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::path::PathBuf;

#[derive(Default)]
pub struct Bans {
    entries: BTreeMap<Ipv4Addr, String>,
    /// The file the list lives in, `<ip> <name>` a line; `None` keeps it
    /// for this run only.
    path: Option<PathBuf>,
    /// Whether retail's file would exist: it was read, or a ban wrote it.
    /// `unbanUser` prints nothing without one.
    file: bool,
}

impl Bans {
    /// Reads `path` if it exists. Lines that do not start with a dotted
    /// quad are skipped, so a `//` comment survives a hand edit but not a
    /// rewrite.
    pub fn load(path: PathBuf) -> std::io::Result<Bans> {
        let (text, file) = match std::fs::read_to_string(&path) {
            Ok(t) => (t, true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (String::new(), false),
            Err(e) => return Err(e),
        };
        let entries = text
            .lines()
            .filter_map(|l| {
                let l = l.trim();
                let (ip, name) = l.split_once(char::is_whitespace).unwrap_or((l, ""));
                Some((ip.parse().ok()?, name.trim().to_string()))
            })
            .collect();
        Ok(Bans {
            entries,
            path: Some(path),
            file,
        })
    }

    pub fn contains(&self, ip: Ipv4Addr) -> bool {
        self.entries.contains_key(&ip)
    }

    /// Adds `ip` under `name` and rewrites the file. A write failure is
    /// logged; the ban still holds for this run.
    pub fn add(&mut self, ip: Ipv4Addr, name: &str) {
        if self.contains(ip) {
            return;
        }
        self.entries.insert(ip, name.to_string());
        self.file = true;
        self.write();
    }

    /// 1.5's `unbanUser` past its usage check (cod_lnxded 1.5b 0x808921d):
    /// every entry whose name is `name` ignoring case goes, and the count
    /// gone comes back. `None` stands for retail's missing `ban.txt`.
    pub fn remove_named(&mut self, name: &str) -> Option<usize> {
        if !self.file {
            return None;
        }
        let before = self.entries.len();
        self.entries.retain(|_, n| !n.eq_ignore_ascii_case(name));
        self.write();
        Some(before - self.entries.len())
    }

    fn write(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let text: String = self
            .entries
            .iter()
            .map(|(ip, name)| match name.as_str() {
                "" => format!("{ip}\n"),
                _ => format!("{ip} {name}\n"),
            })
            .collect();
        if let Err(e) = std::fs::write(path, text) {
            log::error!("writing {}: {e}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_round_trips_and_skips_junk() {
        let dir = std::env::temp_dir().join(format!("vcod-bans-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ban.txt");
        std::fs::write(&path, "// banned\n10.1.2.3\nnot an ip\n10.1.2.4  Some One\r\n").unwrap();
        let mut bans = Bans::load(path.clone()).unwrap();
        assert!(bans.contains(Ipv4Addr::new(10, 1, 2, 3)));
        bans.add(Ipv4Addr::new(8, 8, 4, 4), "dup");
        bans.add(Ipv4Addr::new(8, 8, 8, 8), "DUP");
        let mut again = Bans::load(path.clone()).unwrap();
        assert!(again.contains(Ipv4Addr::new(8, 8, 4, 4)));
        assert!(again.contains(Ipv4Addr::new(10, 1, 2, 3)));
        assert_eq!(again.remove_named("some one"), Some(1));
        assert_eq!(again.remove_named("dup"), Some(2));
        assert_eq!(again.remove_named("dup"), Some(0));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "10.1.2.3\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Retail's `unbanUser` prints nothing when there is no `ban.txt`, and
    /// the first ban makes one.
    #[test]
    fn unban_without_a_file_reports_none() {
        let mut bans = Bans::default();
        assert_eq!(bans.remove_named("x"), None);
        bans.add(Ipv4Addr::new(203, 0, 113, 7), "x");
        assert_eq!(bans.remove_named("x"), Some(1));
        assert_eq!(bans.remove_named("x"), Some(0));
    }
}
