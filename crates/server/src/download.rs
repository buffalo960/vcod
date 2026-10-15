//! Serving a pak to a client: `SV_WriteDownloadToClient` (cod_lnxded
//! 0x8086290) and the `nextdl` / `retransdl` handlers
//! (docs/research/cod11-server-handshake.md, "Serving a download"), and on
//! protocols 6 and 22 the www redirect (docs/research/cod15-protocol-diff.md
//! 14).

use vcod_common::net::msg::MsgWriter;

/// `svc_download`.
const SVC_DOWNLOAD: u8 = 6;
/// lnxded 1.1d reads its blocks 0x800 bytes at a time.
pub const BLOCK: usize = 2048;
/// Blocks in flight before an ack.
const WINDOW: u32 = 8;
/// Unacked blocks go again after this long.
const RETRANSMIT_MS: i32 = 1000;

/// Why a download is refused, the localized key the client shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// `EXE_AUTODL_SERVERDISABLED` (`_PURE` on a pure server).
    Disabled { pure: bool },
    /// `EXE_AUTODL_FILENOTONSERVER`.
    NotFound,
}

impl Refusal {
    fn key(self) -> &'static str {
        match self {
            Refusal::Disabled { pure: false } => "EXE_AUTODL_SERVERDISABLED",
            Refusal::Disabled { pure: true } => "EXE_AUTODL_SERVERDISABLED_PURE",
            Refusal::NotFound => "EXE_AUTODL_FILENOTONSERVER",
        }
    }
}

/// `sv_wwwDownload` on, for a protocol that has the redirect: where the
/// paks are (`sv_wwwBaseURL`) and whether the client fetches them off the
/// server (`sv_wwwDlDisconnected`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WwwConfig {
    pub base_url: String,
    pub disconnected: bool,
}

/// One client's www state (UO coduo_lnxded client +0x10c08, +0x10c0c,
/// +0x10c10). Retail keeps it on the client, across transfers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WwwState {
    /// A redirect went out; `wwwdl` is refused before one.
    pub redirected: bool,
    /// `wwwdl ack` came back: nothing more is written for the transfer.
    pub acked: bool,
    /// The last www transfer failed (`wwwdl fail`, `chkfail`): the next one
    /// goes over UDP.
    pub fallback: bool,
}

/// What a `wwwdl` subcommand does (`SV_WWWDownload`, UO 0x808c298).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WwwReply {
    /// Nothing more to do.
    None,
    /// The transfer is over; close it.
    Close,
    /// Close it and resend the gamestate (`fail`, `chkfail`).
    CloseAndGamestate,
    /// Drop the client with this reason.
    Drop(&'static str),
}

/// `SV_WWWDownload` (UO coduo_lnxded 0x808c298; 1.5b has the same strings):
/// any subcommand before a redirect, an unknown one, or anything but
/// `ack` and `bbl8r` before the ack drops the client.
pub fn wwwdl(state: &mut WwwState, sub: &str, client: &str, download: &str) -> WwwReply {
    const UNEXPECTED: &str = "GMI_EXE_UNEXPECTEDWWWDOWLOADMESSAGE";
    if !state.redirected {
        log::warn!("SV_WWWDownload: unexpected wwwdl '{sub}' for client '{client}'");
        return WwwReply::Drop(UNEXPECTED);
    }
    let is = |s: &str| sub.eq_ignore_ascii_case(s);
    if is("ack") {
        if state.acked {
            log::warn!("WARNING: dupe wwwdl ack from client '{client}'");
        }
        state.acked = true;
        return WwwReply::None;
    }
    if is("bbl8r") {
        return WwwReply::Drop("GMI_EXE_DOWNLOADDISCONNECTED");
    }
    if !state.acked {
        log::warn!("SV_WWWDownload: unexpected wwwdl '{sub}' for client '{client}'");
        return WwwReply::Drop(UNEXPECTED);
    }
    if is("done") {
        state.acked = false;
        return WwwReply::Close;
    }
    if is("chkfail") {
        log::warn!(
            "WARNING: client '{client}' reports that the redirect download for '{download}' \
             had wrong checksum.\n         you should check your download redirect configuration."
        );
    } else if !is("fail") {
        log::warn!("SV_WWWDownload: unknown wwwdl subcommand '{sub}' for client '{client}'");
        return WwwReply::Drop(UNEXPECTED);
    }
    state.acked = false;
    state.fallback = true;
    WwwReply::CloseAndGamestate
}

/// The redirect: `svc_download`, block -1, `<base>/<name>`, the file's size
/// and the flags, bit 0 the disconnected mode (UO 0x808c559).
fn write_redirect(w: &mut MsgWriter, url: &str, size: i32, disconnected: bool) {
    w.write_byte(SVC_DOWNLOAD);
    w.write_short(-1);
    w.write_string(url);
    w.write_long(size);
    w.write_long(i32::from(disconnected));
}

/// What a `nextdl` did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NextDl {
    Acked,
    /// The empty end block was acked; the transfer is over.
    Completed,
    /// Any block but the one due, which retail drops as `broken download`.
    Broken,
}

/// One client's transfer. Block `i` is bytes `i * BLOCK ..` of the file, and
/// the block past the last byte is the empty one that ends it.
pub struct ServerDownload {
    /// As the client asked, e.g. `main/foo.pk3`.
    pub name: String,
    /// `None` until the first write opens it.
    data: Option<Vec<u8>>,
    /// `downloadClientBlock`: the next block the client has to ack.
    acked: u32,
    /// `downloadCurrentBlock`: the next block to put on the wire.
    current: u32,
    /// `downloadSendTime`, the server clock of the last send or ack.
    send_time: i32,
    /// Blocks have started going out over UDP; no redirect follows.
    streaming: bool,
}

impl ServerDownload {
    /// `SV_BeginDownload_f`: the name only; the file opens on the next write.
    pub fn new(name: &str) -> Self {
        ServerDownload {
            name: name.chars().take(63).collect(),
            data: None,
            acked: 0,
            current: 0,
            send_time: 0,
            streaming: false,
        }
    }

    fn blocks(&self) -> u32 {
        self.data
            .as_ref()
            .map_or(0, |d| d.len().div_ceil(BLOCK) as u32 + 1)
    }

    fn block(&self, i: u32) -> &[u8] {
        let d = self.data.as_deref().unwrap_or_default();
        let start = (i as usize * BLOCK).min(d.len());
        &d[start..(start + BLOCK).min(d.len())]
    }

    /// `SV_WriteDownloadToClient`: opens the file on the first call (`open`
    /// answers with its bytes or the refusal), then writes up to `budget`
    /// blocks inside the send window. False when the transfer is over
    /// (refused), so the caller drops it.
    ///
    /// With `www` set and a client that sent `cl_wwwDownload 1`, the
    /// redirect goes out instead, on every message until `wwwdl ack`, and
    /// nothing after it. A client without `cl_wwwDownload`, or one whose
    /// last www transfer failed, gets the blocks: 1.5b's order, which opens
    /// the pak before the www branch (UO's stalls the first and redirects
    /// the second again; cod15-protocol-diff.md 14).
    #[allow(clippy::too_many_arguments)]
    pub fn write(
        &mut self,
        w: &mut MsgWriter,
        now_ms: i32,
        budget: u32,
        open: impl FnOnce(&str) -> Result<Vec<u8>, Refusal>,
        www: Option<&WwwConfig>,
        wants_www: bool,
        state: &mut WwwState,
    ) -> bool {
        if state.acked {
            return true;
        }
        if self.data.is_none() {
            match open(&self.name) {
                Ok(bytes) => self.data = Some(bytes),
                Err(why) => {
                    log::info!("clientDownload: {:?} refused: {why:?}", self.name);
                    w.write_byte(SVC_DOWNLOAD);
                    w.write_short(0);
                    w.write_long(-1);
                    w.write_string(&format!("{}\x15{}", why.key(), self.name));
                    return false;
                }
            }
        }
        if !self.streaming {
            if let Some(www) = www.filter(|_| wants_www) {
                if !state.fallback {
                    let url = format!("{}/{}", www.base_url, self.name);
                    if !state.redirected {
                        log::info!("Redirecting client to {url}");
                    }
                    let size = self.data.as_ref().map_or(0, |d| d.len() as i32);
                    write_redirect(w, &url, size, www.disconnected);
                    state.redirected = true;
                    return true;
                }
                log::info!(
                    "falling back to regular downloading for failed file {}",
                    self.name
                );
                state.fallback = false;
            }
            log::info!("clientDownload: beginning {:?}", self.name);
            state.redirected = false;
            self.streaming = true;
            self.send_time = now_ms;
        }
        let in_flight = (self.acked + WINDOW).min(self.blocks());
        for _ in 0..budget.max(1) {
            if self.acked == in_flight {
                break;
            }
            if self.current == in_flight {
                if now_ms.wrapping_sub(self.send_time) <= RETRANSMIT_MS {
                    break;
                }
                self.current = self.acked;
            }
            let block = self.block(self.current);
            w.write_byte(SVC_DOWNLOAD);
            w.write_short(self.current as i16);
            if self.current == 0 {
                w.write_long(self.data.as_ref().map_or(0, |d| d.len() as i32));
            }
            w.write_short(block.len() as i16);
            for &b in block {
                w.write_byte(b);
            }
            self.current += 1;
            self.send_time = now_ms;
        }
        true
    }

    /// `SV_NextDownload_f` (0x8086168) for `nextdl <block>`.
    pub fn next_dl(&mut self, block: i32, now_ms: i32) -> NextDl {
        if block != self.acked as i32 || self.data.is_none() {
            return NextDl::Broken;
        }
        if self.block(self.acked).is_empty() {
            log::info!("clientDownload: {:?} completed", self.name);
            return NextDl::Completed;
        }
        self.acked += 1;
        self.send_time = now_ms;
        NextDl::Acked
    }

    /// `SV_RetransmitDownload_f` (0x8087a2c): start over from the block due.
    pub fn retransmit(&mut self, block: i32) {
        if block == self.acked as i32 {
            self.current = self.acked;
        }
    }
}

/// `SV_WriteDownloadToClient`'s blocks per message: the whole 2 KiB units
/// the client's rate allows over one snapshot interval, plus one.
pub fn blocks_per_message(rate: i32, snapshot_ms: i32) -> u32 {
    (rate.saturating_mul(snapshot_ms) / 1000 / BLOCK as i32).max(0) as u32 + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use vcod_common::net::huffman::Huffman;
    use vcod_common::net::msg::MsgReader;

    /// A write with no www redirect.
    fn udp(
        dl: &mut ServerDownload,
        w: &mut MsgWriter,
        now: i32,
        budget: u32,
        open: impl FnOnce(&str) -> Result<Vec<u8>, Refusal>,
    ) -> bool {
        dl.write(w, now, budget, open, None, false, &mut WwwState::default())
    }

    /// The blocks one write put out, as `(block, size, bytes)`.
    fn blocks(ops: &[u8], huff: &Huffman) -> Vec<(i16, i32, Vec<u8>)> {
        let mut r = MsgReader::new(ops, huff);
        let mut out = Vec::new();
        while r.read_byte() == SVC_DOWNLOAD && !r.is_overflowed() {
            let n = r.read_short();
            let size = if n == 0 { r.read_long() } else { 0 };
            if size < 0 {
                out.push((n, size, r.read_string().into_bytes()));
                break;
            }
            let len = r.read_short();
            out.push((n, size, (0..len).map(|_| r.read_byte()).collect()));
        }
        out
    }

    #[test]
    fn a_file_goes_out_in_blocks_then_an_empty_one() {
        let huff = Huffman::new();
        let file: Vec<u8> = (0..5000).map(|i| i as u8).collect();
        let mut dl = ServerDownload::new("main/foo.pk3");
        let mut w = MsgWriter::new(&huff);
        assert!(udp(&mut dl, &mut w, 0, 8, |_| Ok(file.clone())));
        let got = blocks(&w.finish(), &huff);
        assert_eq!(got.len(), 4);
        assert_eq!((got[0].0, got[0].1, got[0].2.len()), (0, 5000, BLOCK));
        assert_eq!(got[2].2.len(), 5000 - 2 * BLOCK);
        assert!(got[3].2.is_empty(), "the end block");
        let joined: Vec<u8> = got.iter().flat_map(|b| b.2.clone()).collect();
        assert_eq!(joined, file);
        for n in 0..3 {
            assert_eq!(dl.next_dl(n, 10), NextDl::Acked);
        }
        assert_eq!(dl.next_dl(3, 10), NextDl::Completed);
    }

    #[test]
    fn the_window_waits_for_acks_and_retransmits_after_a_second() {
        let huff = Huffman::new();
        let mut dl = ServerDownload::new("main/big.pk3");
        let mut w = MsgWriter::new(&huff);
        udp(&mut dl, &mut w, 0, 20, |_| Ok(vec![0; 20 * BLOCK]));
        assert_eq!(blocks(&w.finish(), &huff).len(), 8, "one window");
        let mut w = MsgWriter::new(&huff);
        udp(&mut dl, &mut w, 500, 20, |_| unreachable!());
        assert!(blocks(&w.finish(), &huff).is_empty(), "nothing acked yet");
        let mut w = MsgWriter::new(&huff);
        udp(&mut dl, &mut w, 1600, 2, |_| unreachable!());
        let again = blocks(&w.finish(), &huff);
        assert_eq!(again.iter().map(|b| b.0).collect::<Vec<_>>(), [0, 1]);
        assert_eq!(dl.next_dl(5, 1600), NextDl::Broken, "out of order");
    }

    #[test]
    fn a_refusal_names_the_file() {
        let huff = Huffman::new();
        let mut dl = ServerDownload::new("main/zzz_map.pk3");
        let mut w = MsgWriter::new(&huff);
        assert!(!udp(&mut dl, &mut w, 0, 1, |_| Err(Refusal::NotFound)));
        let got = blocks(&w.finish(), &huff);
        assert_eq!(got[0].1, -1);
        assert_eq!(got[0].2, b"EXE_AUTODL_FILENOTONSERVER\x15main/zzz_map.pk3");
    }

    #[test]
    fn the_rate_sets_blocks_per_message() {
        assert_eq!(blocks_per_message(25000, 50), 1);
        assert_eq!(blocks_per_message(99999, 50), 3);
        assert_eq!(blocks_per_message(0, 50), 1);
    }

    fn www(disconnected: bool) -> WwwConfig {
        WwwConfig {
            base_url: "http://127.0.0.1:31072".into(),
            disconnected,
        }
    }

    /// Retail's redirect messages (cod15-protocol-diff.md 14): the 1.5b and
    /// UO servers redirecting `main/nsf_mp_j5outpost.pk3` (953499 bytes),
    /// each fixture the message's reliable acknowledge then its Huffman
    /// block. Ours writes the same block.
    #[test]
    fn the_redirect_is_retails_byte_for_byte() {
        let huff = Huffman::new();
        for (file, disconnected) in [
            ("net/www-redirect-uo.bin", false),
            ("net/www-redirect-uo-discon.bin", true),
            ("net/www-redirect-1.5.bin", false),
        ] {
            let retail = vcod_common::testing::fixture(file);
            let mut dl = ServerDownload::new("main/nsf_mp_j5outpost.pk3");
            let mut state = WwwState::default();
            let mut w = MsgWriter::new(&huff);
            let open = |_: &str| Ok(vec![0; 953499]);
            assert!(dl.write(
                &mut w,
                0,
                4,
                open,
                Some(&www(disconnected)),
                true,
                &mut state
            ));
            w.write_byte(vcod_common::net::msg::SVC_EOF);
            assert_eq!(w.finish(), retail[4..], "{file}");
            assert!(state.redirected && !state.acked);
        }
    }

    #[test]
    fn the_redirect_repeats_until_the_ack_and_then_nothing_goes_out() {
        let huff = Huffman::new();
        let cfg = www(false);
        let mut dl = ServerDownload::new("main/foo.pk3");
        let mut state = WwwState::default();
        for _ in 0..2 {
            let mut w = MsgWriter::new(&huff);
            dl.write(
                &mut w,
                0,
                4,
                |_| Ok(vec![1; 10]),
                Some(&cfg),
                true,
                &mut state,
            );
            let mut r = MsgReader::new(&w.finish(), &huff);
            assert_eq!((r.read_byte(), r.read_short()), (SVC_DOWNLOAD, -1));
            assert_eq!(r.read_string(), "http://127.0.0.1:31072/main/foo.pk3");
            assert_eq!((r.read_long(), r.read_long()), (10, 0));
        }
        assert_eq!(
            wwwdl(&mut state, "ACK", "vcod", "main/foo.pk3"),
            WwwReply::None
        );
        let mut w = MsgWriter::new(&huff);
        assert!(dl.write(
            &mut w,
            0,
            4,
            |_| unreachable!(),
            Some(&cfg),
            true,
            &mut state
        ));
        assert_eq!(w.bits_written(), 0);
        assert_eq!(
            wwwdl(&mut state, "done", "vcod", "main/foo.pk3"),
            WwwReply::Close
        );
        assert!(!state.acked && !state.fallback);
    }

    #[test]
    fn a_client_without_cl_wwwdownload_or_after_a_failure_gets_blocks() {
        let huff = Huffman::new();
        let cfg = www(false);
        let mut dl = ServerDownload::new("main/foo.pk3");
        let mut state = WwwState::default();
        let mut w = MsgWriter::new(&huff);
        dl.write(
            &mut w,
            0,
            4,
            |_| Ok(vec![1; 10]),
            Some(&cfg),
            false,
            &mut state,
        );
        assert_eq!(blocks(&w.finish(), &huff)[0].1, 10, "block 0 with the size");

        // `fail` after the ack: the gamestate again, and the next request
        // streams.
        let mut state = WwwState {
            redirected: true,
            acked: true,
            fallback: false,
        };
        assert_eq!(
            wwwdl(&mut state, "chkfail", "vcod", "main/foo.pk3"),
            WwwReply::CloseAndGamestate
        );
        assert!(state.fallback);
        let mut dl = ServerDownload::new("main/foo.pk3");
        let mut w = MsgWriter::new(&huff);
        dl.write(
            &mut w,
            0,
            4,
            |_| Ok(vec![1; 10]),
            Some(&cfg),
            true,
            &mut state,
        );
        assert_eq!(blocks(&w.finish(), &huff)[0].1, 10);
        assert_eq!(state, WwwState::default());
    }

    #[test]
    fn wwwdl_out_of_turn_drops_the_client() {
        let unexpected = WwwReply::Drop("GMI_EXE_UNEXPECTEDWWWDOWLOADMESSAGE");
        let mut state = WwwState::default();
        assert_eq!(
            wwwdl(&mut state, "ack", "c", "f"),
            unexpected,
            "no redirect yet"
        );
        state.redirected = true;
        assert_eq!(
            wwwdl(&mut state, "done", "c", "f"),
            unexpected,
            "before the ack"
        );
        assert_eq!(
            wwwdl(&mut state, "bbl8r", "c", "f"),
            WwwReply::Drop("GMI_EXE_DOWNLOADDISCONNECTED")
        );
        state.acked = true;
        assert_eq!(wwwdl(&mut state, "what", "c", "f"), unexpected, "unknown");
    }
}
