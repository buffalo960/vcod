//	United Offensive's client-reported flame damage (docs/research/
//	coduo-diff.md 19, "The flame report"), for the retail UO 1.51b server
//	only. The first player to spawn is the shooter and gets the
//	flamethrower and a switch to it; once a second player spawns, the
//	shooter is set down on the first deathmatch spawn with a clear line
//	along +x and the victim 100 units along it, and the pair is put back
//	there every 2 s. The damage and killed callbacks are wrapped to log what the
//	engine hands them, as `PROBE damage <time> ...` and `PROBE killed ...`,
//	and each placement as `PROBE place <time> <shooter spot> health <n>`.
//
//	The shooter is `vcod --net-probe --probe-team axis --probe-weapsel`
//	(it takes the switch and pulls the trigger from 15 s), the victim
//	`vcod --net-probe --probe-team allies --probe-flame`, which runs the
//	play client's flame report (recipe in coduo-diff.md 19).

main()
{
	// dm precaches none of it; a weapon handed over unprecached aborts
	// the server.
	precacheItem("flamethrower_mp");
	thread watch();
	maps\mp\gametypes\dm::main();
}

watch()
{
	wait 1;
	level.probe_damage = level.callbackPlayerDamage;
	level.callbackPlayerDamage = ::probe_damage;
	level.probe_killed = level.callbackPlayerKilled;
	level.callbackPlayerKilled = ::probe_killed;
	shooter = undefined;
	for (;;)
	{
		players = getentarray("player", "classname");
		victim = undefined;
		for (i = 0; i < players.size; i++)
		{
			if (!isdefined(players[i].sessionstate))
				continue;
			if (players[i].sessionstate != "playing")
				continue;
			if (!isdefined(shooter))
			{
				shooter = players[i];
				shooter thread arm();
			}
			else if (players[i] != shooter)
				victim = players[i];
		}
		if (isdefined(shooter) && isdefined(victim) && shooter.sessionstate == "playing")
		{
			place(shooter, victim);
			wait 2;
		}
		else
			wait 0.05;
	}
}

arm()
{
	self giveWeapon("flamethrower_mp");
	self giveMaxAmmo("flamethrower_mp");
	// A select sent with the spawn reaches the client before its first
	// playing snapshot, and is dropped.
	wait 1;
	self switchToWeapon("flamethrower_mp");
	logPrint("PROBE weapon flamethrower_mp " + getTime() + "\n");
}

place(shooter, victim)
{
	// The probes send world angles, so the shooter faces world yaw 0: the
	// victim goes 100 units along +x from the first spawn point with a
	// clear line there and floor under it.
	if (!isdefined(level.probe_spot))
	{
		spots = getentarray("mp_deathmatch_spawn", "classname");
		for (k = 0; k < spots.size; k++)
		{
			at = spots[k].origin;
			eye = at + (0, 0, 60);
			ahead = bulletTrace(eye, eye + (160, 0, 0), false, undefined);
			floor = bulletTrace(at + (100, 0, 48), at + (100, 0, -64), false, undefined);
			if (ahead["fraction"] == 1 && floor["fraction"] < 1)
			{
				level.probe_spot = at;
				break;
			}
		}
		if (!isdefined(level.probe_spot))
		{
			logPrint("PROBE no clear spot\n");
			level.probe_spot = shooter.origin;
		}
	}
	shooter setorigin(level.probe_spot);
	victim setorigin(level.probe_spot + (100, 0, 0));
	logPrint("PROBE place " + getTime() + " " + level.probe_spot + " health " + victim.health + "\n");
}

show_ent(e)
{
	if (!isdefined(e))
		return "undefined";
	return "ent" + e getEntityNumber();
}

probe_damage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc)
{
	logPrint("PROBE damage " + getTime() + " victim " + show_ent(self) + " attacker " + show_ent(eAttacker) + " damage " + iDamage + " mod " + sMeansOfDeath + " weapon " + sWeapon + " health " + self.health + "\n");
	[[level.probe_damage]](eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc);
}

probe_killed(eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc)
{
	logPrint("PROBE killed " + getTime() + " victim " + show_ent(self) + " attacker " + show_ent(eAttacker) + " mod " + sMeansOfDeath + " weapon " + sWeapon + "\n");
	[[level.probe_killed]](eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc);
}
