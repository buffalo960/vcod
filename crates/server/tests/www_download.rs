//! The www download redirect on protocol 22 (cod15-protocol-diff.md 14):
//! vcod's client asks the server for a pak, is sent to an HTTP server on
//! loopback, fetches it there and finishes the handshake.

mod common;

use common::{Queues, connect, step};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use vcod_common::net::protocol::PROTOCOL_V22;
use vcod_common::net::{NetEvent, NetState};
use vcod_server::Server;

/// Serves `body` to every connection on a loopback port; returns the port
/// and the request lines it read.
fn http_server(body: Vec<u8>) -> (u16, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    use std::io::{BufRead, Write};
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { return };
            let mut line = String::new();
            let _ = std::io::BufReader::new(&s).read_line(&mut line);
            log.lock().unwrap().push(line.trim_end().to_string());
            let head = format!("HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
            let _ = s.write_all(head.as_bytes());
            let _ = s.write_all(&body);
        }
    });
    (port, seen)
}

#[test]
fn a_redirected_download_lands_over_http_and_the_gamestate_follows() {
    let scratch = std::env::temp_dir().join(format!("vcod-www-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let served = scratch.join("server/main");
    std::fs::create_dir_all(&served).unwrap();
    let pak = vcod_common::testing::pk3_bytes();
    std::fs::write(served.join("zzz_www.pk3"), &pak).unwrap();
    let (port, requests) = http_server(pak.clone());

    let mut sv = Server::new(common::cfg("mp_carentan", "dm"), Instant::now());
    sv.set_protocol(&PROTOCOL_V22);
    sv.test_mount_paks(Rc::new(
        vcod_common::pk3::Pk3Fs::open(&served).expect("the served pak"),
    ));
    sv.set_cvar("sv_wwwDownload", "1");
    sv.set_cvar("sv_wwwBaseURL", &format!("http://127.0.0.1:{port}"));

    let q = Rc::new(RefCell::new(Queues::default()));
    let mut now = Instant::now();
    let mut cl = connect(&mut sv, &q, &mut now);
    assert_eq!(cl.protocol().version, PROTOCOL_V22.version);
    let dest = scratch.join("client/main/zzz_www.pk3");
    cl.begin_download("main/zzz_www.pk3", &dest).unwrap();

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut landed = false;
    while !landed {
        assert!(std::time::Instant::now() < deadline, "no download");
        now += Duration::from_millis(50);
        for e in step(&mut sv, &q, &mut cl, now) {
            match e {
                NetEvent::DownloadComplete(n) => {
                    assert_eq!(n, "main/zzz_www.pk3");
                    landed = true;
                }
                NetEvent::Dropped(r) => panic!("dropped: {r}"),
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(std::fs::read(&dest).unwrap(), pak);
    assert_eq!(
        *requests.lock().unwrap(),
        ["GET /main/zzz_www.pk3 HTTP/1.0"]
    );

    // `wwwdl done`, then `donedl` brings the gamestate again.
    cl.finish_downloads();
    let mut again = false;
    for _ in 0..40 {
        now += Duration::from_millis(50);
        let events = step(&mut sv, &q, &mut cl, now);
        assert!(
            !events.iter().any(|e| matches!(e, NetEvent::Dropped(_))),
            "{events:?}"
        );
        again |= events.contains(&NetEvent::GamestateReady);
    }
    assert!(again, "no gamestate after donedl");
    assert_ne!(cl.state(), NetState::Disconnected);
    let _ = std::fs::remove_dir_all(&scratch);
}
