//	What the snapshot builder sends an eye in a solid leaf, and what the
//	compass's `trap_InSnapshot` makes of a teammate in no cluster and of
//	one `playSound` touched (cod11-hud-protocol.md 9, cod15-protocol-diff.md
//	15). 1.1 and UO, mp_carentan. Slot 0, the compass viewer, stands at
//	(976, -864); slot 1, the subject, at (-992, 2328), out of its PVS, both
//	on allies. The script holds the subject 1000 units under the map (a
//	solid leaf, no cluster) for three seconds, puts him back, then sounds
//	him. Logs:
//
//		PROBE begin <num>
//		PROBE step <name> <time>
//
//	Recipe: `[COD_VERSION=uo] SECS=40 tools/run_probe.sh
//	client-probes/probe_snapcull mp_carentan`, then the viewer `vcod
//	--net-probe <addr> --probe-compass --probe-team allies --probe-secs 30`
//	(UO: `--game-dir <UO install> --mod-dir uo`) and two seconds later the
//	subject `--probe-edge watch --probe-secs 28`. The viewer's `COMPASS`
//	lines and the subject's `EDGE ents` lines are the measurement
//	(crates/server/tests/uocull_ab.rs).

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
	if (n == 1)
		self thread script();
}

Callback_PlayerDisconnect()
{
}

Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc)
{
}

Callback_PlayerKilled(eInflictor, attacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc)
{
}

spot(n)
{
	if (n == 0)
		return (976, -864, 0);
	return (-992, 2328, 0);
}

step(name)
{
	logPrint("PROBE step " + name + " " + getTime() + "\n");
}

script()
{
	wait 4;
	step("quiet");
	wait 3;
	step("solid");
	for (i = 0; i < 60; i++)
	{
		self setOrigin(spot(1) + (0, 0, -1000));
		wait .05;
	}
	step("back");
	self setOrigin(spot(1));
	wait 3;
	step("sound");
	self playSound("MP_bomb_plant");
	wait 3;
	step("done");
}
