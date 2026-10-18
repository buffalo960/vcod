//	A player's rounds, swing and grenade on a trigger_damage
//	(docs/research/cod11-combat.md 14.8). Stock tdm on mp_uo_stanjel under
//	COD_VERSION=uo tools/run_probe.sh, one `--probe-uo-weapon direct` axis
//	client with `--probe-weapon kar98k_mp`. The script holds him on the
//	first clear spot 160 units off the first birdbush, aims him at it every
//	frame and drives his buttons through `iPrintLn("PROBE_IN ...")`; his
//	damage callback takes nothing. Each phase logs its line, then every
//	notify the bush raises:
//
//	PROBE phase <name>
//	PROBE damage <number> <dmg> <attacker> <mod>
//	PROBE trigger <number> <activator> <ms after the last damage>
//	PROBE weapon <current weapon>
//
//	An attacker is `player`, `world` or a classname; a round's or a swing's
//	damage carries no mod (`-`). crates/server/tests/dmgtrig_ab.rs holds
//	retail's lines.

main()
{
	thread run();
	maps\mp\gametypes\tdm::main();
	level.callbackPlayerDamage = ::no_damage;
}

no_damage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc)
{
}

who(e)
{
	if (!isdefined(e))
		return "undefined";
	if (isplayer(e))
		return "player";
	if (e getEntityNumber() == 1022)
		return "world";
	return e.classname;
}

watch_damage()
{
	for (;;)
	{
		self waittill("damage", dmg, attacker, dir, point, mod);
		self.last_damage = gettime();
		m = "-";
		if (isdefined(mod))
			m = mod;
		logPrint("PROBE damage " + self getEntityNumber() + " " + dmg + " " + who(attacker) + " " + m + "\n");
	}
}

watch_trigger()
{
	for (;;)
	{
		self waittill("trigger", other);
		dt = "-";
		if (isdefined(self.last_damage))
			dt = gettime() - self.last_damage;
		logPrint("PROBE trigger " + self getEntityNumber() + " " + who(other) + " " + dt + "\n");
	}
}

in(buttons)
{
	level.probe_player iprintln("PROBE_IN " + buttons + " 0 0");
}

phase(name)
{
	logPrint("PROBE phase " + name + "\n");
}

ground_at(p)
{
	tr = bullettrace(p + (0, 0, 64), p + (0, 0, -256), false, undefined);
	return tr["position"];
}

//	The first of eight spots `dist` off `centre` with level ground and a
//	clear line from the eye to it.
pick(centre, dist)
{
	for (i = 0; i < 8; i++)
	{
		f = anglestoforward((0, i * 45, 0));
		spot = ground_at(centre + (f[0] * dist, f[1] * dist, 0));
		if (bullettrace(spot + (0, 0, 60), centre, false, undefined)["fraction"] == 1)
			return spot;
	}
	return ground_at(centre + (dist, 0, 0));
}

aim_from(spot, at)
{
	return vectortoangles(at - (spot + (0, 0, 60)));
}

hold()
{
	for (;;)
	{
		self setorigin(level.spot);
		self setplayerangles(level.aim);
		wait 0.05;
	}
}

run()
{
	wait 1;
	t = getentarray("trigger_damage", "classname");
	for (i = 0; i < t.size; i++)
	{
		t[i] thread watch_damage();
		t[i] thread watch_trigger();
	}
	bush = t[0];
	for (;;)
	{
		pl = getentarray("player", "classname");
		if (pl.size > 0 && pl[0].sessionstate == "playing")
			break;
		wait 0.05;
	}
	p = pl[0];
	level.probe_player = p;
	centre = bush.origin + (4, -8, 8);
	level.spot = pick(centre, 160);
	level.aim = aim_from(level.spot, centre);
	p thread hold();
	in(0);
	wait 2;

	phase("shot");
	in(1);
	wait 0.1;
	in(0);
	wait 2;

	phase("shot_again");
	in(1);
	wait 0.1;
	in(0);
	wait 2;

	phase("miss");
	level.aim = level.aim + (0, 40, 0);
	wait 0.5;
	in(1);
	wait 0.1;
	in(0);
	wait 2;

	phase("melee");
	level.spot = pick(centre, 90);
	level.aim = aim_from(level.spot, centre);
	wait 1;
	in(32);
	wait 0.1;
	in(0);
	wait 2;

	phase("grenade");
	level.spot = pick(centre, 110);
	level.aim = aim_from(level.spot, centre) + (40, 0, 0);
	p switchtoweapon("stielhandgranate_mp");
	wait 2;
	logPrint("PROBE weapon " + p getcurrentweapon() + "\n");
	in(1);
	wait 0.4;
	in(0);
	wait 6;
	logPrint("PROBE done\n");
}
