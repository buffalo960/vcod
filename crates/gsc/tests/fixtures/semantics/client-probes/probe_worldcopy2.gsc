//	A turned world copy and a damaged func_static, for
//	docs/research/cod11-movers.md 21. mp_carentan only. Waits for one
//	playing client, stands it at S on the north street and drops a health
//	pack beside it, then spawns a script_brushmodel at O, 64 west of S and low
//	enough that the copy's hill (the world's terrain about its origin, the
//	player 64 out along local x) sits 8 under the street. The copy rises 16
//	through the player and the pack, turns 45 and 45 more in yaw, back 90,
//	pitches 5 and back, sinks 16 and is deleted. Every server frame of a phase
//	logs:
//
//		PROBE f <phase> <time> <player origin> <pack origin> <copy origin> <copy angles>
//
//	and at each rest a 3 x 3 grid of bulletTraces straight down round S:
//
//		PROBE tr <tag> <dx> <dy> <position> <entnum or -1>
//
//	Then three func_statics at the world origin, spawnflags 0, 2 and 4; the
//	second is moved 64 up by an origin write, so a bulletTrace 64 west of S
//	sees its copy while it is linked. Two radiusDamage blasts in the sky
//	inside every copy's link box, then one on the street 200 east of S, each
//	followed by every static's health and the trace; a waiter on each logs
//	its damage, death and trigger notifies:
//
//		PROBE static <tag> <num> health <health>
//		PROBE ptr <tag> <time> <position> <entnum or -1>
//		PROBE notify <tag> <event> <time> <args>
//
//	Run by tools/run_probe.sh client-probes/probe_worldcopy2 mp_carentan
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
	s = (-512, 2688, -16);
	p = undefined;
	for (i = 0; i < 600 && !isdefined(p); i++)
	{
		wait 0.05;
		players = getentarray("player", "classname");
		for (j = 0; j < players.size; j++)
			if (players[j] playing())
				p = players[j];
	}
	if (!isdefined(p))
	{
		logPrint("PROBE no_player\n");
		return;
	}
	wait 2;
	p setorigin(s);
	p setplayerangles((0, 90, 0));
	h = spawn("item_health", s + (40, 0, 0));
	wait 1;
	logPrint("PROBE player " + p getEntityNumber() + " pack " + h getEntityNumber() + "\n");
	sample("rest", p, h, undefined, 0.5);
	grid("bare", s);

	e = spawn("script_brushmodel", (-576, 2688, -75.875));
	logPrint("PROBE copy " + e getEntityNumber() + "\n");
	sample("spawn", p, h, e, 0.5);
	grid("low", s);

	e movez(16, 1);
	sample("up", p, h, e, 2);
	grid("up", s);

	e rotateyaw(45, 1.5);
	sample("yaw45", p, h, e, 2);
	grid("yaw45", s);

	e rotateyaw(45, 1.5);
	sample("yaw90", p, h, e, 2);
	grid("yaw90", s);

	e rotateyaw(-90, 2);
	sample("yaw0", p, h, e, 2.5);

	e rotatepitch(5, 1);
	sample("pitch", p, h, e, 1.5);
	grid("pitch", s);

	e rotatepitch(-5, 1);
	sample("unpitch", p, h, e, 1.5);

	e movez(-16, 1);
	sample("down", p, h, e, 1.5);
	e delete();
	sample("delete", p, h, undefined, 0.5);

	f0 = spawn("func_static", (0, 0, 0));
	f2 = spawn("func_static", (0, 0, 0), 2);
	f4 = spawn("func_static", (0, 0, 0), 4);
	f0 thread notifies("f0");
	f2 thread notifies("f2");
	f4 thread notifies("f4");
	statics("spawned", f0, f2, f4);
	f2.origin = (0, 0, 64);
	logPrint("PROBE static f2 origin " + f2.origin + "\n");
	under = s + (-64, 0, 0);
	ptr("f2_moved", under);
	wait 0.05;
	ptr("f2_next", under);
	radiusDamage((1152, 2464, 900), 100, 50, 50);
	statics("mid", f0, f2, f4);
	ptr("mid", under);
	wait 0.05;
	ptr("mid_next", under);
	wait 0.5;
	radiusDamage((1152, 2464, 900), 100, 50, 50);
	statics("mid2", f0, f2, f4);
	ptr("mid2", under);
	wait 0.05;
	ptr("mid2_next", under);
	wait 0.5;
	radiusDamage(s + (200, 0, 40), 100, 50, 50);
	statics("street", f0, f2, f4);
	ptr("street", under);
	wait 0.5;
	statics("after", f0, f2, f4);
	ptr("after", under);
	logPrint("PROBE done " + getTime() + "\n");
}

//	Early returns rather than one compound test: sessionstate is undefined
//	before the first spawn.
playing()
{
	if (!isdefined(self.sessionstate))
		return false;
	return self.sessionstate == "playing";
}

sample(phase, p, h, e, secs)
{
	frames = secs * 20;
	for (i = 0; i < frames; i++)
	{
		o = "(none) (none)";
		if (isdefined(e))
			o = "" + e.origin + " " + e.angles;
		logPrint("PROBE f " + phase + " " + getTime() + " " + p.origin + " " + h.origin + " " + o + "\n");
		wait 0.05;
	}
}

grid(tag, s)
{
	for (dy = -64; dy <= 64; dy += 64)
		for (dx = -64; dx <= 64; dx += 64)
		{
			at = s + (dx, dy, 0);
			t = bulletTrace(at + (0, 0, 150), at + (0, 0, -150), false, undefined);
			n = -1;
			if (isdefined(t["entity"]))
				n = t["entity"] getEntityNumber();
			logPrint("PROBE tr " + tag + " " + dx + " " + dy + " " + t["position"] + " " + n + "\n");
		}
}

ptr(tag, at)
{
	t = bulletTrace(at + (0, 0, 150), at + (0, 0, -150), false, undefined);
	n = -1;
	if (isdefined(t["entity"]))
		n = t["entity"] getEntityNumber();
	logPrint("PROBE ptr " + tag + " " + getTime() + " " + t["position"] + " " + n + "\n");
}

statics(tag, f0, f2, f4)
{
	logPrint("PROBE static " + tag + " " + f0 getEntityNumber() + " health " + f0.health + "\n");
	logPrint("PROBE static " + tag + " " + f2 getEntityNumber() + " health " + f2.health + "\n");
	logPrint("PROBE static " + tag + " " + f4 getEntityNumber() + " health " + f4.health + "\n");
}

notifies(tag)
{
	self thread on_death(tag);
	self thread on_trigger(tag);
	for (;;)
	{
		self waittill("damage", amount, attacker);
		n = -1;
		if (isdefined(attacker))
			n = attacker getEntityNumber();
		logPrint("PROBE notify " + tag + " damage " + getTime() + " " + amount + " " + n + "\n");
	}
}

on_death(tag)
{
	for (;;)
	{
		self waittill("death", attacker);
		n = -1;
		if (isdefined(attacker))
			n = attacker getEntityNumber();
		logPrint("PROBE notify " + tag + " death " + getTime() + " " + n + "\n");
	}
}

on_trigger(tag)
{
	for (;;)
	{
		self waittill("trigger", who);
		n = -1;
		if (isdefined(who))
			n = who getEntityNumber();
		logPrint("PROBE notify " + tag + " trigger " + getTime() + " " + n + "\n");
	}
}
