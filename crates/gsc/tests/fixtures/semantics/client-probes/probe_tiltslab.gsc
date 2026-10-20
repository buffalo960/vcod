//	A player's capsule against a pitched and a rolled submodel, for
//	docs/research/cod11-movers.md 22. mp_harbor only. The bel gametype's
//	script_brushmodel *9 is one brush, a slab 4 thick along x, 56 along y,
//	96 tall, its centre B = (-9638, -8003, 56) with its entity at the world
//	origin. The probe keeps it from _gameobjects and, for each pose, turns
//	it and moves it so its centre lands over C = (-11000, -7656), an open
//	floor at z 0 (origin O = C' - R B, R the pose's AnglesToAxis, the
//	origins precomputed below). The player is stood in the air at z 110,
//	falls onto the slab's upward face and settles:
//
//		flat	(90, 0, 0): lying, 96 along x, 56 along y, top at 32
//		edge	(0, 0, 90): on its side, 4 along x, 96 along y, top at 57
//		slope	(60, 0, 0): its -x face 30 degrees off level, facing -x
//		roll	(0, 0, -60): its -y face 30 degrees off level, facing -y
//
//	Every server frame of a pose logs
//
//		PROBE f <pose> <time> <player origin> <slab origin> <slab angles>
//		PROBE g <pose> <time> <player x - C.x> <player y - C.y> <player z> <ground>
//
//	the second's floats printed by the engine's %g, six digits.
//	Run by tools/run_probe.sh client-probes/probe_tiltslab mp_harbor with a
//	--probe-team allies client.

main()
{
	thread run();
	maps\mp\gametypes\dm::main();
}

run()
{
	if (getcvar("mapname") != "mp_harbor")
	{
		logPrint("PROBE unsupported " + getcvar("mapname") + "\n");
		return;
	}
	// Before dm's _gameobjects pass deletes it.
	slab = undefined;
	ents = getentarray("script_brushmodel", "classname");
	for (i = 0; i < ents.size; i++)
		if (isdefined(ents[i].script_gameobjectname) && ents[i].script_gameobjectname == "bel")
			slab = ents[i];
	slab.script_gameobjectname = undefined;

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
	cx = -11000;
	cy = -7656;

	pose(p, slab, cx, cy, "flat", (90, 0, 0), (-11056, 347, -9608), (10, 5, 110));
	pose(p, slab, cx, cy, "edge", (0, 0, 90), (-1362, -7600, 8032), (0, 10, 110));
	pose(p, slab, cx, cy, "slope", (60, 0, 0), (-6229.4974, 347, -8347.7528), (-15, 0, 110));
	pose(p, slab, cx, cy, "roll", (0, 0, -60), (-1362, -3702.9974, -6908.8013), (0, -20, 110));
	logPrint("PROBE done " + getTime() + "\n");
}

//	Stands the player at `drop` off C first and lets two frames of cmds
//	take it off whatever it stood on, so the slab's move carries no rider.
pose(p, slab, cx, cy, name, angles, origin, drop)
{
	p setorigin((cx + drop[0], cy + drop[1], drop[2]));
	p setplayerangles((0, 0, 0));
	wait 0.1;
	slab moveto(origin, 0.05);
	slab rotateto(angles, 0.05);
	wait 0.1;
	logPrint("PROBE slab " + name + " " + slab.origin + " " + slab.angles + "\n");
	// The pose's last bits: origin off its target in ten-thousandths,
	// angles in millionths of a degree.
	o = slab.origin;
	a = slab.angles;
	logPrint("PROBE r " + name + " " + ((o[0] - origin[0]) * 10000) + " " + ((o[1] - origin[1]) * 10000) + " " + ((o[2] - origin[2]) * 10000) + " " + ((a[0] - angles[0]) * 1000000) + " " + ((a[1] - angles[1]) * 1000000) + " " + ((a[2] - angles[2]) * 1000000) + "\n");
	sample(name, p, slab, cx, cy, 2);
}

//	Early returns rather than one compound test: sessionstate is undefined
//	before the first spawn.
playing()
{
	if (!isdefined(self.sessionstate))
		return false;
	return self.sessionstate == "playing";
}

sample(name, p, e, cx, cy, secs)
{
	frames = secs * 20;
	for (i = 0; i < frames; i++)
	{
		logPrint("PROBE f " + name + " " + getTime() + " " + p.origin + " " + e.origin + " " + e.angles + "\n");
		logPrint("PROBE g " + name + " " + getTime() + " " + (p.origin[0] - cx) + " " + (p.origin[1] - cy) + " " + p.origin[2] + " " + p isOnGround() + "\n");
		wait 0.05;
	}
}
