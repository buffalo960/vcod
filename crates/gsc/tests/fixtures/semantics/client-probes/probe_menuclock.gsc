//	The clock a spawn woken by a menu answer runs on, against the client's
//	first playing snapshot, and a spawn off a callback's `wait`
//	(docs/research/cod11-gametypes-re-bel.md 8.7). One `--net-probe
//	--save-scripted` client sits as a spectator until its `mr`, whose
//	waiter spawns it; each `kill` respawns it half a second later from
//	the callback. Each spawn logs `PROBE spawned` and, a `wait 0.05` after
//	its "spawned" notify, `PROBE placed`, as `probe_bel` does. See this
//	directory's README.md, "probe_menuclock".

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
	level.spot = getentarray("mp_deathmatch_spawn", "classname")[0];
	self.pers["team"] = "allies";
	self.sessionteam = "spectator";
	self.sessionstate = "spectator";
	self spawn(level.spot.origin + (0, 0, 64), level.spot.angles);
	logPrint("PROBE begin " + self getEntityNumber() + " " + getTime() + "\n");
	self thread log_spawns();
	self thread place_on_spawn();
	self waittill("menuresponse", menu, response);
	self spawn_playing();
}

spawn_playing()
{
	self.sessionteam = "allies";
	self.sessionstate = "playing";
	self.maxhealth = 100;
	self.health = 100;
	self spawn(level.spot.origin, level.spot.angles);
	self notify("spawned");
}

log_spawns()
{
	for (;;)
	{
		self waittill("spawned");
		logPrint("PROBE spawned " + self getEntityNumber() + " " + self.pers["team"] + " " + getTime() + " " + self.origin + "\n");
	}
}

place_on_spawn()
{
	for (;;)
	{
		self waittill("spawned");
		wait 0.05;
		logPrint("PROBE placed " + self getEntityNumber() + " " + self.pers["team"] + " " + getTime() + "\n");
	}
}

Callback_PlayerDisconnect() {}
Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc) {}

Callback_PlayerKilled(eInflictor, attacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc)
{
	logPrint("PROBE killed " + self getEntityNumber() + " " + getTime() + "\n");
	self.sessionstate = "dead";
	wait 0.5;
	self spawn_playing();
}
