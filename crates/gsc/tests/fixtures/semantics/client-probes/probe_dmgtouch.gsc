//	A grenade's flight and a shell's impact on a trigger_damage
//	(docs/research/cod11-combat.md 14.8): `G_GrenadeTouchTriggerDamage`
//	once `enableGrenadeTouchDamage` opened the bush, and UO's
//	`G_MissileImpact` walk. Stock tdm on mp_uo_stanjel under
//	COD_VERSION=uo tools/run_probe.sh, one `--probe-uo-weapon direct` axis
//	client with `--probe-weapon kar98k_mp`, driven as probe_dmgshot drives
//	him. Two frags fly through the first bush, the walk opened for the
//	first only; then a panzerschreck shell from 120 units off strikes the
//	ground inside it, and one strikes the ground 300 units the other way.
//	Each phase logs its line, then every notify a bush raises:
//
//	PROBE phase <name>
//	PROBE damage <number> <dmg> <attacker> <mod>
//	PROBE trigger <number> <activator> <ms after the last damage>
//	PROBE weapon <current weapon>
//	PROBE nade <origin>      (the grenade's last origin before it went off)
//	PROBE rocket <origin>    (the rocket's last origin before it went off)
//
//	An attacker is `player`, `world` or a classname. crates/server/tests/
//	dmgtrig_ab.rs holds retail's lines.

main()
{
	precacheItem("panzerschreck_mp");
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

//	The last origin of the first `classname` entity before it goes away.
watch_last(classname, tag)
{
	last = undefined;
	for (;;)
	{
		g = getentarray(classname, "classname");
		if (g.size > 0)
			last = g[0].origin;
		else if (isdefined(last))
		{
			logPrint("PROBE " + tag + " " + last + "\n");
			return;
		}
		wait 0.05;
	}
}

ground_at(p)
{
	tr = bullettrace(p + (0, 0, 64), p + (0, 0, -256), false, undefined);
	return tr["position"];
}

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

throw()
{
	thread watch_last("grenade", "nade");
	in(1);
	wait 0.4;
	in(0);
	wait 6;
}

//	A bush, the surface the ray from the world origin out through it meets
//	450 to 1500 units past it, and a spot 220 short of that surface which
//	sees it: a shell striking there walks a segment from its contact to the
//	struck face's normal, which passes through the bush, and its splash
//	(256) does not reach it.
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
	level.spot = pick(centre, 110);
	level.aim = aim_from(level.spot, centre);
	p thread hold();
	in(0);
	p giveMaxAmmo("stielhandgranate_mp");
	p switchtoweapon("stielhandgranate_mp");
	wait 2;
	logPrint("PROBE weapon " + p getcurrentweapon() + "\n");

	phase("touch");
	bush enableGrenadeTouchDamage();
	throw();

	phase("touch_off");
	bush disableGrenadeTouchDamage();
	throw();

	phase("shell");
	level.spot = pick(centre, 120);
	ground = ground_at(centre);
	level.aim = aim_from(level.spot, ground);
	logPrint("PROBE spot " + level.spot + " " + ground + " " + level.aim + "\n");
	p giveWeapon("panzerschreck_mp");
	p giveMaxAmmo("panzerschreck_mp");
	p switchtoweapon("panzerschreck_mp");
	wait 3;
	logPrint("PROBE weapon " + p getcurrentweapon() + "\n");
	logPrint("PROBE aim " + level.aim + " " + p.origin + " " + p.angles + "\n");
	thread watch_last("rocket", "rocket");
	in(1);
	wait 0.1;
	in(0);
	wait 3;

	phase("shell_wide");
	level.aim = aim_from(level.spot, ground_at(level.spot + (level.spot - ground)));
	wait 8;
	logPrint("PROBE aim " + level.aim + " " + p.origin + " " + p.angles + "\n");
	thread watch_last("rocket", "rocket");
	in(1);
	wait 0.1;
	in(0);
	wait 3;
	logPrint("PROBE done\n");
}
