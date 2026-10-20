//	The order threads woken from inside one client packet run in: the
//	`menuresponse` waiters a `mr` wakes, the level waiter a `kill`'s
//	`Callback_PlayerKilled` wakes, and the threads that callback starts,
//	when two commands ride one packet (docs/research/cod11-gsc-language.md,
//	"The packet pass"). One plain `--net-probe` client sends the commands
//	in pairs with one `--probe-say` time each. Every line carries
//	`getTime()` and the last time a ticker, queued a frame earlier, ran.
//	A second client sending the same commands at the same moments puts two
//	packets in one frame. See this directory's README.md, "probe_threadorder".

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
	precacheMenu("probe_menu_zero");
	level.lasttick = -1;
}

Callback_PlayerConnect()
{
	self waittill("begin");
	level.spot = getentarray("mp_deathmatch_spawn", "classname")[0];
	self.sessionteam = "allies";
	self.sessionstate = "playing";
	self.maxhealth = 100;
	self.health = 100;
	self spawn(level.spot.origin, level.spot.angles);
	logPrint("PROBE begin " + self getEntityNumber() + "\n");
	n = self getEntityNumber();
	if(n == 0)
		level thread ticker();
	self thread waiter("w1." + n);
	self thread waiter("w2." + n);
	level thread levelwaiter("w3." + n);
	self thread waiter("w4." + n);
}

ticker()
{
	for(;;)
	{
		wait 0.05;
		level.lasttick = getTime();
	}
}

note(tag)
{
	logPrint("PROBE " + tag + " t " + getTime() + " tick " + level.lasttick + "\n");
}

waiter(name)
{
	for(;;)
	{
		self waittill("menuresponse", menu, response);
		note(name + " " + response);
		wait 0;
		note(name + "z " + response);
	}
}

levelwaiter(name)
{
	for(;;)
	{
		level waittill("killed");
		note(name);
		wait 0;
		note(name + "z");
	}
}

killedthread()
{
	n = self getEntityNumber();
	note("kt." + n);
	wait 0;
	note("ktz." + n);
}

Callback_PlayerDisconnect() {}
Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc) {}

Callback_PlayerKilled(eInflictor, attacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc)
{
	n = self getEntityNumber();
	note("killed." + n);
	level notify("killed");
	self thread killedthread();
	note("killed_after." + n);
	wait 0;
	note("killedz." + n);
	wait 0.5;
	self.sessionstate = "playing";
	self.health = 100;
	self spawn(level.spot.origin, level.spot.angles);
	note("respawn." + n);
}
