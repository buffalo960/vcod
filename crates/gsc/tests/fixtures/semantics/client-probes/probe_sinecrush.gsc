//	G_MoverPush's TR_SINE arm, for docs/research/cod11-movers.md 21.
//	mp_carentan only. Spawns a func_bobbing on the first frame, whose copy of
//	the world swings 32 up and down about the world's own place (section 18),
//	and stands the first playing client at S, a spot under a ceiling 96 over
//	its floor, at 2400 or the first frame after it is found. Sinking, the
//	copy's ceiling presses the player into the real floor, where the push
//	fails. The level drops 4 s after the spawn, so the client has to be
//	playing by 2.4 s of level time: start it straight after the server.
//	Every server frame from then on logs
//
//		PROBE f <time> <player origin> <health> <sessionstate> <bobbing origin>
//
//	and the damage and killed callbacks log what the engine hands them. The
//	drop (Reached_BinaryMover: bad moverState) ends the capture.
//	Run by tools/run_probe.sh client-probes/probe_sinecrush mp_carentan
//	with a --probe-team allies client.

main()
{
	thread run();
	maps\mp\gametypes\dm::main();
}

run()
{
	if (getcvar("mapname") != "mp_carentan")
	{
		logPrint("PROBE unsupported " + getcvar("mapname") + "\n");
		return;
	}
	s = (752, -40, -7.75);
	wait 0.05;
	level.probe_damage = level.callbackPlayerDamage;
	level.callbackPlayerDamage = ::probe_damage;
	level.probe_killed = level.callbackPlayerKilled;
	level.callbackPlayerKilled = ::probe_killed;
	b = spawn("func_bobbing", (0, 0, 0));
	logPrint("PROBE bobbing " + b getEntityNumber() + " " + getTime() + "\n");
	p = undefined;
	for (;;)
	{
		if (!isdefined(p) && getTime() >= 2400)
		{
			players = getentarray("player", "classname");
			for (j = 0; j < players.size; j++)
				if (players[j] playing())
					p = players[j];
			if (isdefined(p))
			{
				p setorigin(s);
				p setplayerangles((0, 90, 0));
				logPrint("PROBE player " + p getEntityNumber() + " " + getTime() + "\n");
			}
		}
		if (isdefined(p))
			logPrint("PROBE f " + getTime() + " " + p.origin + " " + p.health + " " + p.sessionstate + " " + b.origin + "\n");
		wait 0.05;
	}
}

//	Early returns rather than one compound test: sessionstate is undefined
//	before the first spawn.
playing()
{
	if (!isdefined(self.sessionstate))
		return false;
	return self.sessionstate == "playing";
}

show_ent(e)
{
	if (!isdefined(e))
		return "undefined";
	return "ent" + e getEntityNumber();
}

probe_damage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc)
{
	logPrint("PROBE damage " + getTime() + " inflictor " + show_ent(eInflictor) + " attacker " + show_ent(eAttacker) + " damage " + iDamage + " dflags " + iDFlags + " mod " + sMeansOfDeath + " weapon " + sWeapon + " hitloc " + sHitLoc + " health " + self.health + "\n");
	[[level.probe_damage]](eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc);
}

probe_killed(eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc)
{
	logPrint("PROBE killed " + getTime() + " inflictor " + show_ent(eInflictor) + " attacker " + show_ent(eAttacker) + " damage " + iDamage + " mod " + sMeansOfDeath + " weapon " + sWeapon + " hitloc " + sHitLoc + "\n");
	[[level.probe_killed]](eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc);
}
