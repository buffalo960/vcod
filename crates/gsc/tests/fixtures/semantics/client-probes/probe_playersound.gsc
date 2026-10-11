//	Whether a player that playSound touched rides every snapshot past the
//	PVS cull, the S&D planter's case (sd.gsc's `other playsound
//	("MP_bomb_plant")`; playSound sets SVF_BROADCAST, game.mp 0x5d957), and
//	what ends it. mp_carentan. Slot 0, the viewer, stands at the south end;
//	slot 1, the sounder, at the north end, out of the viewer's PVS. The
//	sounder plays the sound, moves, dies, respawns, turns spectator, plays
//	again, and the map restarts (map_restart(true) reruns ClientConnect,
//	which stores svFlags 0x200 at game.mp 0x4258f). Logs:
//
//		PROBE begin <num> <phase>
//		PROBE step <name> <time>
//
//	Run by tools/run_probe.sh with two `--net-probe --probe-edge watch`
//	clients, the viewer first; the viewer's `EDGE ents` lines say when
//	entity 1 rode its snapshots.

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
	if (getcvar("probe_phase") == "")
		setcvar("probe_phase", "0");
}

Callback_PlayerConnect()
{
	self waittill("begin");
	n = self getEntityNumber();
	logPrint("PROBE begin " + n + " " + getcvar("probe_phase") + "\n");
	self.sessionteam = "allies";
	self place(n);
	if (n == 1)
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
	self.sessionstate = "dead";
}

place(n)
{
	self.sessionstate = "playing";
	self.maxhealth = 100;
	self.health = 100;
	self spawn(spot(n), (0, 90, 0));
}

// `probe_pair` picks the two spots; 0 is the pair the capture used.
spot(n)
{
	v[0] = (976, -864, 0);
	s[0] = (-992, 2328, 0);
	v[1] = (-1176, 1560, 24);
	s[1] = (1960, 2304, -16);
	v[2] = (296, -1280, 32);
	s[2] = (-568, 2744, -16);
	v[3] = (1872, 936, -8);
	s[3] = (-632, 1712, 8);
	i = getcvarint("probe_pair");
	if (n == 0)
		return v[i];
	return s[i];
}

step(name)
{
	logPrint("PROBE step " + name + " " + getTime() + "\n");
}

script()
{
	if (getcvar("probe_phase") == "1")
	{
		wait 3;
		step("after_restart");
		wait 3;
		step("done");
		return;
	}
	wait 4;
	step("quiet");
	wait 3;
	step("sound");
	self playSound("MP_bomb_plant");
	wait 3;
	step("move");
	self setOrigin(spot(1) + (0, 64, 0));
	wait 3;
	step("die");
	self suicide();
	wait 3;
	step("respawn");
	self place(1);
	wait 3;
	step("spectator");
	self.sessionstate = "spectator";
	self spawn(spot(1) + (0, 0, 100), (0, 90, 0));
	wait 3;
	step("playing");
	self place(1);
	wait 3;
	step("restart");
	setcvar("probe_phase", "1");
	map_restart(true);
}
