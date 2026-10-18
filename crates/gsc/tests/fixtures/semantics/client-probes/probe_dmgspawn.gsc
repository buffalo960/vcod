//	A script-spawned trigger_damage (docs/research/cod11-combat.md 14.8):
//	`spawn("trigger_damage", origin)` runs `SP_trigger_damage` with no
//	brush model key, so it links inline model 0, a copy of the whole world
//	where it stands. No client. Stock tdm under tools/run_probe.sh on
//	mp_carentan: the world box below is that map's model 0
//	(-8513 -6721 -637.12274)..(10817 11649 2369), and `mid` is the
//	trigger's box centre, which `CanDamage` probes from.
//
//	PROBE spawned <classname> <health> <origin>
//	PROBE phase <time> <name> / PROBE health <time> <health>
//	PROBE damage <dmg> <mod> <time> / PROBE trigger <time>

main()
{
	thread run();
	maps\mp\gametypes\tdm::main();
}

watch()
{
	self endon("death");
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
	self endon("death");
	for (;;)
	{
		self waittill("trigger", other);
		logPrint("PROBE trigger " + gettime() + "\n");
	}
}

blast(t, name, at)
{
	logPrint("PROBE phase " + gettime() + " " + name + "\n");
	radiusDamage(at, 64, 100, 50);
	logPrint("PROBE health " + gettime() + " " + t.health + "\n");
	wait 1;
}

run()
{
	wait 1;
	o = (0, 0, 0);
	sp = getentarray("mp_teamdeathmatch_spawn", "classname");
	if (sp.size > 0)
		o = sp[0].origin;
	centre = (1152, 2464, 865.93863);
	t = spawn("trigger_damage", o);
	logPrint("PROBE spawned " + t.classname + " " + t.health + " " + t.origin + "\n");
	t thread watch();
	t thread watch_trigger();
	wait 0.5;
	// At the origin: inside the box, the line to its centre blocked.
	blast(t, "origin", o + (0, 0, 32));
	// In the sky above the origin, inside the box, clear to the centre.
	blast(t, "above", o + (0, 0, 2000));
	// At the centre itself: every probe is a point.
	blast(t, "centre", o + centre);
	// 40 under the centre: clear, inside the box.
	blast(t, "under_centre", o + centre - (0, 0, 40));
	// 70 past the box's top face over the centre: past the radius.
	blast(t, "past_top", o + (centre[0], centre[1], 2369 + 1 + 70));
	// Moved 1500 down: the centre goes under the ground, the old centre
	// in the sky is cut off from it, and the new one is a point in solid.
	t.origin = o - (0, 0, 1500);
	blast(t, "moved_old_centre", o + centre);
	blast(t, "moved_new_centre", o + centre - (0, 0, 1500));
	t.origin = o;
	blast(t, "back", o + centre);
	t delete();
	logPrint("PROBE phase " + gettime() + " deleted\n");
	radiusDamage(o + centre, 64, 100, 50);
	wait 1;
	logPrint("PROBE done\n");
}
