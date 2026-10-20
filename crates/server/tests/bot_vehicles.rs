//! Bots in United Offensive vehicles (`bots::vehicle`): a bot set down
//! beside mp_cassino's tanks walks up to one and boards it with the use
//! key, drives it along the graph, and gets out again. Needs `COD_DIR` at a
//! UO install; each test builds mp_cassino's graph, so they are ignored and
//! run in release.

use std::rc::Rc;
use std::time::{Duration, Instant};

const MAP: &str = "mp_cassino";
const FRAME: Duration = Duration::from_millis(50);

fn uo_server(bots: usize, shoot: bool) -> Option<(vcod_server::Server, Instant)> {
    let fs = vcod_common::testing::uo_fs()?;
    let bsp = vcod_common::bsp::parse(&fs.read(&fs.resolve_map(MAP).unwrap()).unwrap())
        .expect("parse the bsp");
    let now = Instant::now();
    let cfg = vcod_server::ServerConfig {
        map: MAP.into(),
        hostname: "vcod test".into(),
        max_clients: 8,
        gametype: "tdm".into(),
        test_entities: 0,
        trace: false,
        bots,
        bots_shoot: shoot,
    };
    let mut sv = vcod_server::Server::new(cfg, now);
    sv.set_protocol(&vcod_common::net::protocol::PROTOCOL_V22);
    sv.load_world(vcod_server::world::World::from_bsp(&bsp, Some(&fs)));
    sv.load_scripts(Rc::new(fs)).expect("load the scripts");
    Some((sv, now))
}

fn run(sv: &mut vcod_server::Server, now: &mut Instant, ticks: usize) {
    for _ in 0..ticks {
        *now += FRAME;
        sv.tick(*now);
    }
}

/// Sets the bot down on the ground beside the hull of `spot`, off its
/// left side.
fn beside(sv: &mut vcod_server::Server, slot: usize, spot: &vcod_server::bots::VehicleSpot) {
    let (s, c) = spot.yaw.to_radians().sin_cos();
    let off = spot.maxs[1] + 70.0;
    let at = [
        spot.origin[0] - s * off,
        spot.origin[1] + c * off,
        spot.origin[2] + 64.0,
    ];
    let ground = sv.test_ground_under(at).expect("ground beside the tank");
    sv.place_client(slot, [ground[0], ground[1], ground[2] + 0.125], spot.yaw);
}

/// The bot's seat, if it sits in one, run tick by tick up to `ticks`.
fn run_until_seated(
    sv: &mut vcod_server::Server,
    now: &mut Instant,
    slot: usize,
    ticks: usize,
) -> Option<(u32, i32)> {
    (0..ticks).find_map(|_| {
        run(sv, now, 1);
        sv.bot_body(slot)?.seat
    })
}

/// A lone bot (nobody to fight) set down beside a tank boards it within
/// 10 s, in the driver's seat.
#[test]
#[ignore = "builds mp_cassino's UO graph, ~5 s in release; run with --ignored"]
fn a_bot_beside_a_tank_boards_it() {
    let Some((mut sv, mut now)) = uo_server(1, false) else {
        eprintln!("COD_DIR has no uo/: skipping");
        return;
    };
    run(&mut sv, &mut now, 100);
    let slot = sv.bot_slots()[0];
    assert!(sv.bot_body(slot).unwrap().playing, "the bot never spawned");
    let tanks = sv.test_vehicles();
    assert!(!tanks.is_empty(), "mp_cassino's tdm has no vehicle");
    beside(&mut sv, slot, &tanks[0]);
    let seat = run_until_seated(&mut sv, &mut now, slot, 200);
    assert_eq!(
        seat,
        Some((tanks[0].num, 1)),
        "the bot never took the tank's driver's seat"
    );
    assert_eq!(sv.script_aborts(), Vec::<String>::new());
}

/// Once in, the bot drives the tank off along the graph toward its roam,
/// and backs off and gets out when it is stuck or the ride is long.
#[test]
#[ignore = "builds mp_cassino's UO graph and runs a bot 100 s of game time, ~10 s in release; run with --ignored"]
fn a_bot_drives_a_tank_off_and_gets_out() {
    let Some((mut sv, mut now)) = uo_server(1, false) else {
        eprintln!("COD_DIR has no uo/: skipping");
        return;
    };
    run(&mut sv, &mut now, 100);
    let slot = sv.bot_slots()[0];
    let tank = sv.test_vehicles()[0];
    beside(&mut sv, slot, &tank);
    let Some((num, 1)) = run_until_seated(&mut sv, &mut now, slot, 200) else {
        panic!("the bot never took the driver's seat");
    };
    let mut far = 0.0f32;
    let mut out = None;
    for tick in 0..2000 {
        run(&mut sv, &mut now, 1);
        let at = sv
            .test_vehicles()
            .into_iter()
            .find(|v| v.num == num)
            .expect("the tank is gone")
            .origin;
        far = far.max((at[0] - tank.origin[0]).hypot(at[1] - tank.origin[1]));
        if std::env::var("VEH_REPORT").is_ok() && tick % 20 == 0 {
            eprintln!(
                "{tick} tank {at:?} seat {:?}",
                sv.bot_body(slot).unwrap().seat
            );
        }
        if sv.bot_body(slot).unwrap().seat.is_none() {
            out = Some(tick);
            break;
        }
    }
    eprintln!("drove {far:.0} units, out at tick {out:?}");
    assert!(far > 500.0, "the tank went only {far:.0} units");
    assert!(out.is_some(), "the bot never got out");
    assert!(sv.bot_body(slot).unwrap().playing);
    assert_eq!(sv.script_aborts(), Vec::<String>::new());
}
