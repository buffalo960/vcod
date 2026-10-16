//	The origin and angles a script reads straight after `spawn`, against
//	the spot it passed (cod11-gametypes-re-bel.md 8.5): mp_carentan's first
//	deathmatch spot with health 0 and 100, the same spot 100 up, and a
//	pitched, rolled spot facing 90, then a spectator's and an intermission
//	camera's spawn straight after a player's. Coordinates print times 1000, as %g.
//	Run: tools/run_probe.sh client-probes/probe_spawnview mp_carentan, and
//	vcod --net-probe <server> --probe-secs 12 once it is up.

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

Callback_PlayerDisconnect()
{
}

Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc)
{
}

Callback_PlayerKilled(eInflictor, attacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc)
{
}

milli(v)
{
	return "" + v[0] * 1000 + " " + v[1] * 1000 + " " + v[2] * 1000;
}

look(tag, o, a)
{
	self.sessionstate = "playing";
	self spawn(o, a);
	logPrint("PROBE " + tag + " spot " + milli(o) + " / " + milli(a) + "\n");
	logPrint("PROBE " + tag + " origin " + milli(self.origin) + "\n");
	logPrint("PROBE " + tag + " angles " + milli(self.angles) + "\n");
	wait 0.05;
	logPrint("PROBE " + tag + " next origin " + milli(self.origin) + "\n");
	logPrint("PROBE " + tag + " next angles " + milli(self.angles) + "\n");
	wait 1;
}

Callback_PlayerConnect()
{
	wait 3;
	spawns = getentarray("mp_deathmatch_spawn", "classname");
	o = spawns[0].origin;
	look("dead", o, spawns[0].angles);
	self.health = 100;
	look("live", o, spawns[0].angles);
	look("high", o + (0, 0, 100), spawns[0].angles);
	look("turned", o, (10, 90, 5));
	look("back", o, (-30, -135, 0));
	heights = [];
	heights[0] = 1;
	heights[1] = 2;
	heights[2] = 4;
	heights[3] = 8;
	heights[4] = 16;
	heights[5] = -1;
	heights[6] = -2;
	heights[7] = -4;
	for (i = 0; i < heights.size; i++)
	{
		self spawn(o + (0, 0, heights[i]), spawns[0].angles);
		logPrint("PROBE drop " + heights[i] + " origin " + milli(self.origin) + "\n");
		wait 0.2;
	}
	// A spectator's and an intermission camera's spawn straight after a
	// player's, in one frame: what their angles keep.
	self.sessionstate = "playing";
	self spawn(o, (0, 90, 0));
	logPrint("PROBE pre-spec angles " + milli(self.angles) + "\n");
	self.sessionstate = "spectator";
	self spawn(o + (0, 0, 64), (20, 45, 0));
	logPrint("PROBE spec origin " + milli(self.origin) + "\n");
	logPrint("PROBE spec angles " + milli(self.angles) + "\n");
	wait 0.2;
	self.sessionstate = "playing";
	self spawn(o, (0, 90, 0));
	self.sessionstate = "intermission";
	self spawn(o + (0, 0, 64), (20, 45, 0));
	logPrint("PROBE inter origin " + milli(self.origin) + "\n");
	logPrint("PROBE inter angles " + milli(self.angles) + "\n");
}
