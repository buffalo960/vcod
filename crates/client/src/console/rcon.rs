//! `rcon` (CoDMP.exe 0x40f880; 1.5 0x410490, UO 0x4111c0): the console
//! line sent to a server as the OOB `rcon <rconPassword> <args>`, and the
//! `print` packets it answers with (docs/research/cod11-console.md 7).

use std::net::{SocketAddrV4, UdpSocket};

use vcod_common::net::connectionless;

/// Retail's message buffer, the four 0xff bytes included.
const MESSAGE_MAX: usize = 0x400;

pub const NOT_CONNECTED: &str =
    "You must either be connected,\nor set the 'rconAddress' cvar\nto issue rcon commands";
pub const TOO_LONG: &str = "rcon command too long";

/// The datagram for `rcon <args>`: `rcon `, the password as it stands (an
/// empty one too), then each argument after a space, quoted when it is
/// empty or holds a byte at or below a space, and the terminating NUL.
/// `None` when it fills retail's buffer.
pub fn packet(password: &str, args: &[String]) -> Option<Vec<u8>> {
    let mut p = connectionless::build_oob("rcon ");
    p.extend_from_slice(password.as_bytes());
    for a in args {
        p.push(b' ');
        let quote = a.is_empty() || a.bytes().any(|b| b <= b' ');
        if quote {
            p.push(b'"');
        }
        p.extend_from_slice(a.as_bytes());
        if quote {
            p.push(b'"');
        }
    }
    if p.len() >= MESSAGE_MAX {
        return None;
    }
    p.push(0);
    Some(p)
}

/// The text of an OOB `print` packet, as `CL_ConnectionlessPacket` prints
/// it (0x410db1), its trailing newlines dropped for the console's lines.
pub fn print_text(pkt: &[u8]) -> Option<String> {
    let (cmd, rest) = connectionless::parse_oob(pkt)?;
    (cmd == "print").then(|| {
        String::from_utf8_lossy(rest)
            .trim_end_matches(['\n', '\0'])
            .to_string()
    })
}

/// The socket an rcon goes out on while not connected (to `rconAddress`);
/// connected, the connection's own carries it and its replies.
#[derive(Default)]
pub struct RconSocket {
    sock: Option<UdpSocket>,
}

impl RconSocket {
    pub fn send(&mut self, to: SocketAddrV4, packet: &[u8]) {
        if self.sock.is_none() {
            self.sock = UdpSocket::bind("0.0.0.0:0")
                .and_then(|s| s.set_nonblocking(true).map(|()| s))
                .map_err(|e| log::warn!("rcon: cannot open a socket: {e}"))
                .ok();
        }
        if let Some(s) = &self.sock
            && let Err(e) = s.send_to(packet, to)
        {
            log::warn!("rcon: sending to {to}: {e}");
        }
    }

    /// Every `print` that arrived since the last poll, whoever sent it.
    pub fn poll(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        let Some(s) = &self.sock else { return out };
        let mut buf = [0u8; 0x4000];
        while let Ok((n, _)) = s.recv_from(&mut buf) {
            out.extend(print_text(&buf[..n]).filter(|t| !t.is_empty()));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::shell::{Effect, Shell};

    fn rcon_args(shell: &mut Shell, line: &str) -> Vec<String> {
        match shell.execute(line).as_slice() {
            [Effect::Rcon(args)] => args.clone(),
            other => panic!("{other:?}"),
        }
    }

    /// The packet off the cvar and the typed line: arguments rebuilt one
    /// space apart, quoted where retail quotes them.
    #[test]
    fn the_packet_carries_the_password_and_the_rebuilt_line() {
        let mut shell = Shell::new();
        shell.execute("rconpassword secret");
        let args = rcon_args(&mut shell, "rcon  set sv_hostname \"a b\" \"\"");
        let pw = shell.cvar("rconPassword").unwrap().to_string();
        assert_eq!(
            packet(&pw, &args).unwrap(),
            b"\xff\xff\xff\xffrcon secret set sv_hostname \"a b\" \"\"\0"
        );
        // Retail sends an empty password as it stands.
        assert_eq!(
            packet("", &["status".into()]).unwrap(),
            b"\xff\xff\xff\xffrcon  status\0"
        );
        // 4 + 5 + 1015 bytes fills the 1 KB buffer; one fewer fits.
        assert!(packet("", &["x".repeat(1014)]).is_none());
        assert_eq!(packet("", &["x".repeat(1013)]).unwrap().len(), 0x400);
    }

    /// The packet against vcod-server's `SVC_RemoteCommand` in process, and
    /// its answer read back as the console's line.
    #[test]
    fn vcod_server_answers_the_packet() {
        let t = std::time::Instant::now();
        let mut sv = vcod_server::Server::new(
            vcod_server::ServerConfig {
                map: "mp_carentan".into(),
                hostname: "vcod test".into(),
                max_clients: 4,
                gametype: "dm".into(),
                test_entities: 0,
                trace: false,
                bots: 0,
                bots_shoot: false,
            },
            t,
        );
        sv.set_cvar("rconPassword", "secret");
        let from = std::net::SocketAddr::from(([127, 0, 0, 1], 9));
        let mut ask = |pw: &str, line: &str, at| {
            let mut shell = Shell::new();
            let args = rcon_args(&mut shell, line);
            sv.handle_packet(from, &packet(pw, &args).unwrap(), at);
            let out = sv.take_outgoing();
            assert_eq!(out.len(), 1);
            assert_eq!(out[0].0, from);
            print_text(&out[0].1).unwrap()
        };
        assert_eq!(ask("wrong", "rcon status", t), "Bad rconpassword.");
        let t = t + std::time::Duration::from_millis(500);
        let status = ask("secret", "rcon status", t);
        assert!(
            status.starts_with("map: mp_carentan\nnum score ping"),
            "{status}"
        );
    }
}
