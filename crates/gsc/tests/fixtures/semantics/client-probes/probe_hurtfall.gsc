//	A trigger_hurt kill in mid-air and the corpse's fall, for
//	docs/research/cod11-combat.md 5.8. mp_carentan only. The map's one
//	trigger_hurt (*1, the kill slab at z -176..-160 under the whole map) is
//	lifted 400 (or probe_lift) by an origin write on the first frame, to
//	224..240, and at 2400 or the first frame after it the first playing
//	client is stood 400 over the street at (900 1930), where it falls into
//	the slab, dies and its corpse falls on to the street. Every server frame from then on logs
//
//		PROBE f <time> <player origin> <health> <sessionstate>
//
//	and the damage and killed callbacks log what the engine hands them.
//	PROBE done 3 s after the kill ends the capture. Under +set probe_lift 120
//	the slab's top is 3 over the street, so the kill comes on the landing;
//	+set probe_late_lift <z> moves the slab again at the drop.
//	+set probe_drop <z> drops from that z instead of 362. +set probe_spawn
//	<spawnflags> spawns a second trigger_hurt by script at the drop, a point
//	at (900 1930 -20) inside the body standing on the street, and +set
//	probe_dmg <n> writes the dmg of that one (or of the slab without
//	probe_spawn) before the drop. Under either, every damage callback also
//	spawns and deletes a script_origin and logs its number (PROBE spawned),
//	and PROBE done comes 6 s after the drop if nothing kills by then.
//	+set probe_use <ms> (with probe_spawn) logs every "trigger" notify on
//	the spawned hurt (PROBE trigger) and calls its useby with the player
//	<ms> and 2 * <ms> after the drop (PROBE use), hurt_use's toggle of its
//	touch. +set probe_write 1 tries a write of the spawned hurt's
//	spawnflags, classname and model at the drop, each in its own thread
//	(PROBE write, then PROBE wrote only if the write did not end it).
//	Run by tools/run_probe.sh client-probes/probe_hurtfall mp_carentan
//	with a --probe-team allies --probe-ride client.

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
	lift = 400;
	if (getcvar("probe_lift") != "")
		lift = getcvarint("probe_lift");
	hurt = getent("trigger_hurt", "classname");
	hurt.origin = (0, 0, lift);
	logPrint("PROBE hurt " + hurt getEntityNumber() + " " + hurt.origin + "\n");
	level.probe_rate = getcvar("probe_dmg") != "" || getcvar("probe_spawn") != "";
	if (getcvar("probe_dmg") != "" && getcvar("probe_spawn") == "")
		hurt.dmg = getcvarint("probe_dmg");
	s = (900, 1930, 362);
	if (getcvar("probe_drop") != "")
		s = (900, 1930, getcvarint("probe_drop"));
	wait 0.05;
	level.probe_damage = level.callbackPlayerDamage;
	level.callbackPlayerDamage = ::probe_damage;
	level.probe_killed = level.callbackPlayerKilled;
	level.callbackPlayerKilled = ::probe_killed;
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
				if (getcvar("probe_late_lift") != "")
				{
					hurt.origin = (0, 0, getcvarint("probe_late_lift"));
					logPrint("PROBE hurt " + hurt getEntityNumber() + " " + hurt.origin + "\n");
				}
				if (getcvar("probe_spawn") != "")
				{
					hurt = spawn("trigger_hurt", (900, 1930, -20), getcvarint("probe_spawn"));
					if (getcvar("probe_dmg") != "")
						hurt.dmg = getcvarint("probe_dmg");
					if (getcvar("probe_use") != "")
					{
						hurt thread probe_triggered();
						hurt thread probe_use(p, getcvarint("probe_use"));
					}
					if (getcvar("probe_write") != "")
					{
						hurt thread probe_write_spawnflags();
						hurt thread probe_write_classname();
						hurt thread probe_write_model();
					}
				}
				if (level.probe_rate)
					logPrint("PROBE fields " + hurt getEntityNumber() + " dmg " + hurt.dmg + " spawnflags " + hurt.spawnflags + "\n");
				logPrint("PROBE player " + p getEntityNumber() + " " + getTime() + "\n");
				level.probe_drop = getTime();
			}
		}
		if (isdefined(p))
		{
			logPrint("PROBE f " + getTime() + " " + p.origin + " " + p.health + " " + p.sessionstate + "\n");
			if (isdefined(level.probe_dead) && getTime() >= level.probe_dead + 3000)
			{
				logPrint("PROBE done " + getTime() + "\n");
				return;
			}
			if (level.probe_rate && getTime() >= level.probe_drop + 6000)
			{
				logPrint("PROBE done " + getTime() + "\n");
				return;
			}
		}
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

show(v)
{
	if (!isdefined(v))
		return "undefined";
	return "" + v;
}

show_ent(e)
{
	if (!isdefined(e))
		return "undefined";
	return "ent" + e getEntityNumber();
}

probe_damage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc)
{
	logPrint("PROBE damage " + getTime() + " inflictor " + show_ent(eInflictor) + " attacker " + show_ent(eAttacker) + " damage " + iDamage + " dflags " + iDFlags + " mod " + sMeansOfDeath + " weapon " + sWeapon + " point " + show(vPoint) + " dir " + show(vDir) + " hitloc " + sHitLoc + " health " + self.health + "\n");
	if (level.probe_rate)
	{
		e = spawn("script_origin", (0, 0, 0));
		logPrint("PROBE spawned " + getTime() + " " + e getEntityNumber() + "\n");
		e delete();
	}
	[[level.probe_damage]](eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc);
}

probe_killed(eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc)
{
	logPrint("PROBE killed " + getTime() + " inflictor " + show_ent(eInflictor) + " attacker " + show_ent(eAttacker) + " damage " + iDamage + " mod " + sMeansOfDeath + " weapon " + sWeapon + " dir " + show(vDir) + " hitloc " + sHitLoc + " origin " + self.origin + "\n");
	level.probe_dead = getTime();
	[[level.probe_killed]](eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc);
}

probe_triggered()
{
	for (;;)
	{
		self waittill("trigger", who);
		logPrint("PROBE trigger " + getTime() + " " + show_ent(who) + "\n");
	}
}

probe_use(p, ms)
{
	wait ms / 1000;
	logPrint("PROBE use " + getTime() + "\n");
	self useby(p);
	wait ms / 1000;
	logPrint("PROBE use " + getTime() + "\n");
	self useby(p);
}

probe_write_spawnflags()
{
	logPrint("PROBE write spawnflags " + getTime() + "\n");
	self.spawnflags = 0;
	logPrint("PROBE wrote spawnflags " + show(self.spawnflags) + "\n");
}

probe_write_classname()
{
	logPrint("PROBE write classname " + getTime() + "\n");
	self.classname = "trigger_multiple";
	logPrint("PROBE wrote classname " + show(self.classname) + "\n");
}

probe_write_model()
{
	logPrint("PROBE write model " + getTime() + "\n");
	self.model = "";
	logPrint("PROBE wrote model " + show(self.model) + "\n");
}
