//	trigger_damage under script blasts (docs/research/cod11-combat.md
//	14.8). Needs no client; mp_uo_stanjel under COD_VERSION=uo
//	tools/run_probe.sh, whose seventeen `birdbush` trigger_damages are the
//	only stock ones with no spawnflags. Each phase logs its line, then
//	every notify the watched triggers raise and their health:
//
//	PROBE count <n>
//	PROBE ent <number> <origin> <health>
//	PROBE phase <time> <name>
//	PROBE damage <time> <number> <dmg> <attacker> <mod>
//	PROBE death <time> <number> <attacker>
//	PROBE trigger <time> <number> <activator>
//	PROBE health <time> <number> <health>
//
//	An attacker is its entity number, `world` for the world.
//	crates/server/tests/dmgtrig_ab.rs holds retail's lines.

main()
{
	level.callbackStartGameType = ::cb;
	level.callbackPlayerConnect = ::cb;
	level.callbackPlayerDisconnect = ::cb;
	level.callbackPlayerDamage = ::cb;
	level.callbackPlayerKilled = ::cb;
	maps\mp\gametypes\_callbacksetup::SetupCallbacks();
	thread go();
}

cb() {}

who(e)
{
	if (!isdefined(e))
		return "undefined";
	if (e getEntityNumber() == 1022)
		return "world";
	return e getEntityNumber();
}

watch_damage()
{
	for (;;)
	{
		self waittill("damage", dmg, attacker, dir, point, mod);
		m = "-";
		if (isdefined(mod))
			m = mod;
		logPrint("PROBE damage " + gettime() + " " + self getEntityNumber() + " " + dmg + " " + who(attacker) + " " + m + "\n");
	}
}

watch_death()
{
	for (;;)
	{
		self waittill("death", attacker);
		logPrint("PROBE death " + gettime() + " " + self getEntityNumber() + " " + who(attacker) + "\n");
	}
}

watch_trigger()
{
	for (;;)
	{
		self waittill("trigger", other);
		logPrint("PROBE trigger " + gettime() + " " + self getEntityNumber() + " " + who(other) + "\n");
	}
}

phase(name)
{
	logPrint("PROBE phase " + gettime() + " " + name + "\n");
}

health(t)
{
	logPrint("PROBE health " + gettime() + " " + t getEntityNumber() + " " + t.health + "\n");
}

go()
{
	wait 1.5;
	t = getentarray("trigger_damage", "classname");
	logPrint("PROBE count " + t.size + "\n");
	for (i = 0; i < t.size; i++)
	{
		logPrint("PROBE ent " + t[i] getEntityNumber() + " " + t[i].origin + " " + t[i].health + "\n");
		t[i] thread watch_damage();
		t[i] thread watch_death();
		t[i] thread watch_trigger();
	}
	a = t[0];
	b = t[1];

	phase("single");
	radiusDamage(a.origin, 200, 100, 50);
	health(a);
	wait 0.1;
	phase("busy");
	radiusDamage(a.origin, 200, 100, 50);
	health(a);
	wait 0.25;
	phase("still_busy");
	radiusDamage(a.origin, 200, 100, 50);
	wait 0.1;
	phase("rearmed");
	radiusDamage(a.origin, 200, 100, 50);
	wait 1;

	phase("edge");
	radiusDamage(a.origin + (0, 0, 150), 200, 100, 50);
	health(a);
	wait 1;

	phase("out_of_range");
	radiusDamage(a.origin + (0, 0, 600), 200, 100, 50);
	wait 1;

	phase("kill");
	radiusDamage(a.origin, 200, 40000, 40000);
	health(a);
	wait 1;

	phase("same_frame");
	radiusDamage(b.origin, 200, 100, 50);
	radiusDamage(b.origin, 200, 100, 50);
	health(b);
	wait 1;

	phase("attacker");
	o = spawn("script_origin", b.origin);
	radiusDamage(b.origin, 200, 100, 50, o, o);
	wait 1;

	phase("gone_attacker");
	o2 = spawn("script_origin", a.origin);
	radiusDamage(a.origin, 200, 100, 50, o2, o2);
	o2 delete();
	wait 1;

	phase("wait_loop");
	for (i = 0; i < 12; i++)
	{
		radiusDamage(b.origin, 200, 100, 50);
		wait 0.05;
	}
	wait 1;
	logPrint("PROBE end\n");
}
