//	mp_foy's belltower `trigger_damage` (spawnflags 115, `script_exploder`
//	2; docs/research/cod11-combat.md 14.8). Stock tdm on mp_foy under
//	COD_VERSION=uo tools/run_probe.sh, one `--probe-uo-weapon direct` axis
//	client with `--probe-weapon kar98k_mp`, driven as probe_dmgshot drives
//	him: a round and a frag at the tower from 600 units off, then a
//	panzerschreck shell from inside the belfry into its floor (a shell
//	from outside sets it off only when the spread lands it inside). Each phase
//	logs its line, then every notify the trigger raises and what the map's
//	`belltower_damage()` and `_load.gsc`'s exploder thread leave behind:
//
//	PROBE phase <name>
//	PROBE damage <dmg> <attacker> <mod>
//	PROBE trigger <activator>
//	PROBE hurt <dmg> <mod> <attacker>   (the player, off the radiusDamage)
//	PROBE state <trigger> <tower brush> <fx models>  (each `1` while it exists)
//	PROBE spot <origin> / PROBE inside <floor> <aim>
//	PROBE rocket <origin> / PROBE nade <origin>
//
//	crates/server/tests/dmgtrig_ab.rs holds retail's lines.

main()
{
	precacheItem("panzerschreck_mp");
	thread run();
	maps\mp\gametypes\tdm::main();
	level.callbackPlayerDamage = ::hurt;
}

hurt(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc)
{
	logPrint("PROBE hurt " + iDamage + " " + sMeansOfDeath + " " + who(eAttacker) + "\n");
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
	self endon("death");
	for (;;)
	{
		self waittill("damage", dmg, attacker, dir, point, mod);
		m = "-";
		if (isdefined(mod))
			m = mod;
		logPrint("PROBE damage " + dmg + " " + who(attacker) + " " + m + "\n");
	}
}

watch_trigger()
{
	self waittill("trigger", other);
	logPrint("PROBE trigger " + who(other) + "\n");
}

state()
{
	tr = 0;
	if (isdefined(getent("belltower_damage", "targetname")))
		tr = 1;
	br = 0;
	if (isdefined(getent("delete", "targetname")))
		br = 1;
	fx = 0;
	m = getentarray("script_model", "classname");
	for (i = 0; i < m.size; i++)
		if (isdefined(m[i].script_exploder) && m[i].script_exploder == 2)
			fx++;
	logPrint("PROBE state " + tr + " " + br + " " + fx + "\n");
}

in(buttons)
{
	level.probe_player iprintln("PROBE_IN " + buttons + " 0 0");
}

phase(name)
{
	logPrint("PROBE phase " + name + "\n");
}

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
	tr = bullettrace(p + (0, 0, 64), p + (0, 0, -1024), false, undefined);
	return tr["position"];
}

//	The first of sixteen spots `dist` off the tower, on the ground, whose
//	eye reaches the tower within 48 units of `at`.
pick(at, dist)
{
	for (i = 0; i < 16; i++)
	{
		f = anglestoforward((0, i * 22.5, 0));
		spot = ground_at(at + (f[0] * dist, f[1] * dist, 0));
		if (distance(bullettrace(spot + (0, 0, 60), at, false, undefined)["position"], at) < 48)
			return spot;
	}
	return undefined;
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
	trig = getent("belltower_damage", "targetname");
	trig thread watch_damage();
	trig thread watch_trigger();
	state();
	for (;;)
	{
		pl = getentarray("player", "classname");
		if (pl.size > 0 && pl[0].sessionstate == "playing")
			break;
		wait 0.05;
	}
	p = pl[0];
	level.probe_player = p;
	at = trig.origin;
	spot = undefined;
	for (d = 600; d <= 1400 && !isdefined(spot); d += 200)
		spot = pick(at, d);
	if (!isdefined(spot))
	{
		logPrint("PROBE no spot\n");
		logPrint("PROBE done\n");
		return;
	}
	logPrint("PROBE spot " + spot + "\n");
	level.spot = spot;
	level.aim = aim_from(spot, at);
	p thread hold();
	in(0);
	wait 2;

	phase("round");
	in(1);
	wait 0.1;
	in(0);
	wait 2;

	phase("frag");
	p giveMaxAmmo("stielhandgranate_mp");
	p switchtoweapon("stielhandgranate_mp");
	wait 2;
	thread watch_last("grenade", "nade");
	in(1);
	wait 0.4;
	in(0);
	wait 6;
	state();

	phase("shell");
	p giveWeapon("panzerschreck_mp");
	p giveMaxAmmo("panzerschreck_mp");
	p switchtoweapon("panzerschreck_mp");
	wait 3;
	logPrint("PROBE weapon " + p getcurrentweapon() + "\n");
	floor = bullettrace(at, at - (0, 0, 512), false, undefined)["position"];
	level.spot = floor;
	level.aim = aim_from(floor, floor + (0, 48, 0));
	logPrint("PROBE inside " + floor + " " + level.aim + "\n");
	wait 2;
	thread watch_last("rocket", "rocket");
	in(1);
	wait 0.1;
	in(0);
	wait 3;
	state();
	logPrint("PROBE done\n");
}
