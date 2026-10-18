//	A script-spawned trigger_damage (docs/research/cod11-combat.md 14.8):
//	`spawn("trigger_damage", origin)` runs `SP_trigger_damage` with no
//	brush model key. No client. Stock tdm on any map under
//	tools/run_probe.sh.
//
//	PROBE spawned <classname> <health> <origin>
//	PROBE damage <dmg> <mod> / PROBE trigger <activator>

main()
{
	thread run();
	maps\mp\gametypes\tdm::main();
}

watch()
{
	for (;;)
	{
		self waittill("damage", dmg, attacker, dir, point, mod);
		m = "-";
		if (isdefined(mod))
			m = mod;
		logPrint("PROBE damage " + dmg + " " + m + " " + gettime() + "\n");
	}
}

watch_trigger()
{
	for (;;)
	{
		self waittill("trigger", other);
		logPrint("PROBE trigger " + gettime() + "\n");
	}
}

run()
{
	wait 1;
	o = (0, 0, 0);
	sp = getentarray("mp_teamdeathmatch_spawn", "classname");
	if (sp.size > 0)
		o = sp[0].origin;
	t = spawn("trigger_damage", o);
	logPrint("PROBE spawned " + t.classname + " " + t.health + " " + t.origin + "\n");
	t thread watch();
	t thread watch_trigger();
	wait 0.5;
	radiusDamage(o + (0, 0, 32), 64, 100, 50);
	logPrint("PROBE after " + t.health + " " + gettime() + "\n");
	wait 1;
	radiusDamage(o + (0, 0, 2000), 64, 100, 50);
	wait 1;
	logPrint("PROBE done\n");
}
