//	A player's rounds on a script-spawned trigger_damage
//	(docs/research/cod11-combat.md 14.8), a copy of the whole world moved to
//	its origin. Stock tdm on mp_carentan under tools/run_probe.sh, one
//	`--probe-uo-weapon direct` axis client with `--probe-weapon
//	kar98k_mp`. The script holds him on the first tdm spawn point's ground,
//	aims him every frame at the ground 160 units off along the first
//	heading with level ground and a clear line, and drives his trigger
//	through `iPrintLn("PROBE_IN ...")`; his damage callback takes nothing.
//	The round strikes the ground; the trigger's origin moves the copy's
//	ground under or over the round's end:
//
//	PROBE phase <name> <trigger origin>
//	PROBE damage <dmg> <attacker> <mod>
//	PROBE trigger <activator>

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
	self endon("death");
	for (;;)
	{
		self waittill("trigger", other);
		logPrint("PROBE trigger " + who(other) + "\n");
	}
}

in(buttons)
{
	level.probe_player iprintln("PROBE_IN " + buttons + " 0 0");
}

ground_at(p)
{
	tr = bullettrace(p + (0, 0, 64), p + (0, 0, -256), false, undefined);
	return tr["position"];
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

shot(t, name, at)
{
	t.origin = at;
	wait 0.5;
	logPrint("PROBE phase " + name + " " + t.origin + "\n");
	in(1);
	wait 0.1;
	in(0);
	wait 2;
}

run()
{
	wait 1;
	sp = getentarray("mp_teamdeathmatch_spawn", "classname");
	spot = ground_at(sp[0].origin);
	target = undefined;
	for (i = 0; i < 8 && !isdefined(target); i++)
	{
		f = anglestoforward((0, i * 45, 0));
		g = ground_at(spot + (f[0] * 160, f[1] * 160, 0));
		if (absf(g[2] - spot[2]) < 1 && bullettrace(spot + (0, 0, 60), g + (0, 0, 1), false, undefined)["fraction"] == 1)
			target = g;
	}
	logPrint("PROBE site " + spot + " " + target + "\n");
	for (;;)
	{
		pl = getentarray("player", "classname");
		if (pl.size > 0 && pl[0].sessionstate == "playing")
			break;
		wait 0.05;
	}
	p = pl[0];
	level.probe_player = p;
	level.spot = spot;
	level.aim = vectortoangles(target - (spot + (0, 0, 60)));
	p thread hold();
	in(0);
	t = spawn("trigger_damage", (0, 0, 20));
	t thread watch_damage();
	t thread watch_trigger();
	wait 2;
	// The copy's ground 20 over the world's: the round ends inside it.
	shot(t, "up", (0, 0, 20));
	// 20 under: the round ends in the air over it.
	shot(t, "down", (0, 0, -20));
	// On the world: the round ends on the copy's ground.
	shot(t, "level", (0, 0, 0));
	// 1 over and 1 under.
	shot(t, "up1", (0, 0, 1));
	shot(t, "down1", (0, 0, -1));
	logPrint("PROBE done\n");
}

absf(x)
{
	if (x < 0)
		return 0 - x;
	return x;
}
