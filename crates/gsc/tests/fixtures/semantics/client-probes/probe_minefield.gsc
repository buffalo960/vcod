//	What the wire carries when a minefield kills its player, issue #4. The
//	stock maps\mp\_minefields::minefield_kill runs `self playsound
//	("minefield_click")`, waits, then `self playsound("explo_mine")`,
//	playfx and radiusDamage(origin, 300, 2000, 50) on the same frame, which
//	kills the player that made the sound. This replays those calls on slot
//	1 (the victim) with slot 0 (the watcher) standing beside it, out of the
//	blast's 300 range. mp_pavlov, from the tdm spawn points. Steps:
//
//		click     minefield_click
//		alive     explo_mine + playfx, no damage (the victim survives)
//		dead      explo_mine + playfx + radiusDamage, as the stock script
//
//	With `+set probe_field 1` the victim is instead put inside mp_pavlov's
//	minefield `*5` 4 s after it begins, and the stock map script does the
//	rest.
//
//	With `+set probe_model 1` a script_model with no model is spawned
//	beside the watcher 4 s after the victim begins, sounded 3 s later and
//	deleted 3 s after that: when a modelless script_model rides a
//	snapshot (1.5's and UO's minefields sound their explosion on one, in
//	playSoundinSpace).
//
//	Logs:
//
//		PROBE begin <num>
//		PROBE step <name> <time>
//
//	Run by tools/run_probe.sh with two plain `--net-probe` clients, the
//	watcher first; each prints `EV_SOUND_ALIAS` lines from both rings.

main()
{
	level.callbackStartGameType = ::Callback_StartGameType;
	level.callbackPlayerConnect = ::Callback_PlayerConnect;
	level.callbackPlayerDisconnect = ::Callback_PlayerDisconnect;
	level.callbackPlayerDamage = ::Callback_PlayerDamage;
	level.callbackPlayerKilled = ::Callback_PlayerKilled;

	maps\mp\gametypes\_callbacksetup::SetupCallbacks();
}

Callback_StartGameType()
{
	level._effect["probe_mine"] = loadfx("fx/impacts/newimps/minefield.efx");
}

Callback_PlayerConnect()
{
	self waittill("begin");
	n = self getEntityNumber();
	logPrint("PROBE begin " + n + "\n");
	self.sessionteam = "allies";
	self.sessionstate = "playing";
	self.maxhealth = 100;
	self.health = 100;
	self spawn(spot(n), (0, 90, 0));
	if (n == 1 && getcvar("probe_model") == "1")
		self thread model();
	else if (n == 1 && getcvar("probe_field") == "1")
		self thread field();
	else if (n == 1)
		self thread script();
}

Callback_PlayerDisconnect()
{
}

Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc)
{
	self finishPlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc);
}

Callback_PlayerKilled(eInflictor, attacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc)
{
	step("killed " + sMeansOfDeath);
	self.sessionstate = "dead";
	body = self cloneplayer();
}

// Slot 1 at the first tdm spawn, slot 0 400 units east of it.
spot(n)
{
	s = getentarray("mp_teamdeathmatch_spawn", "classname");
	o = s[0].origin;
	if (n == 0)
		return o + (400, 0, 0);
	return o;
}

step(name)
{
	logPrint("PROBE step " + name + " " + getTime() + "\n");
}

script()
{
	wait 4;
	step("click");
	self playsound("minefield_click");
	wait 3;
	step("alive");
	origin = self getorigin();
	self playsound("explo_mine");
	playfx(level._effect["probe_mine"], origin);
	wait 3;
	step("dead");
	origin = self getorigin();
	self playsound("explo_mine");
	playfx(level._effect["probe_mine"], origin);
	radiusDamage(origin, 300, 2000, 50);
	wait 3;
	step("done");
}

// The middle of `*5`, (-8128, 7736)..(-7680, 8896); the body falls to the
// ground.
field()
{
	wait 4;
	step("field");
	self setOrigin((-7904, 8316, 64));
	wait 6;
	step("done");
}

model()
{
	wait 4;
	step("spawn");
	org = spawn("script_model", spot(0) + (64, 0, 0));
	logPrint("PROBE model " + org getEntityNumber() + "\n");
	wait 3;
	step("sound");
	org playsound("explo_mine");
	wait 3;
	step("delete");
	org delete();
	wait 3;
	step("done");
}
