//	A player's capsule against a rolled submodel's non-axial side, for
//	docs/research/cod11-movers.md 22. mp_depot only. Its script_brushmodel
//	*3 (the last script_brushmodel) carries a ramp: brushes 3, 4 and 5
//	share the non-axial top side n = (0, -0.196, 0.981), d = -38.44, over
//	x -1434..-1258, y 374..407. A roll turns the capsule's sphere offset
//	into the model's frame as (f.z, -l.z, u.z) * offset; the brush arm
//	picks the sphere nearer the side off that offset's sign, so how deep
//	the capsule sits on the ramp reads the -l.z. For each pose the probe
//	turns the model and moves it so the ramp point B = (-1310, y, on the
//	side) lands at C = (-2624, -264, 16), an open floor at z -24
//	(origin O = C - R B, R the pose's AnglesToAxis, precomputed below),
//	and drops the player onto it from z 120:
//
//		down	(0, 0, -25), B.y 390: the ramp 13.7 degrees off level
//		steep	(0, 0, -40), B.y 392: 28.7 degrees off level
//		turned	(-10, 90, -30), B.y 390: 21.1 degrees off level, facing
//			-x and -y
//
//	Every server frame of a pose logs
//
//		PROBE f <pose> <time> <player origin> <model origin> <model angles>
//		PROBE g <pose> <time> <player x - C.x> <player y - C.y> <player z> <ground>
//
//	the second's floats printed by the engine's %g, six digits.
//	Run by tools/run_probe.sh client-probes/probe_rollramp mp_depot with a
//	--probe-team allies --probe-ride client.

main()
{
	thread run();
	maps\mp\gametypes\dm::main();
}

run()
{
	if (getcvar("mapname") != "mp_depot")
	{
		logPrint("PROBE unsupported " + getcvar("mapname") + "\n");
		return;
	}
	ents = getentarray("script_brushmodel", "classname");
	logPrint("PROBE models " + ents.size + "\n");
	ramp = ents[ents.size - 1];

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
	cx = -2624;
	cy = -264;

	pose(p, ramp, cx, cy, "down", (0, 0, -25), (-1314, -633.8576, 145.6564), (0, -6, 120));
	pose(p, ramp, cx, cy, "steep", (0, 0, -40), (-1314, -589.4867, 237.9438), (0, -8, 120));
	pose(p, ramp, cx, cy, "turned", (-10, 90, -30), (-2266.8501, 998.0717, 402.4253), (6, 3, 120));
	logPrint("PROBE done " + getTime() + "\n");
}

//	Stands the player at `drop` off C first and lets two frames of cmds
//	take it off whatever it stood on, so the model's move carries no rider.
pose(p, ramp, cx, cy, name, angles, origin, drop)
{
	p setorigin((cx + drop[0], cy + drop[1], drop[2]));
	p setplayerangles((0, 0, 0));
	wait 0.1;
	ramp moveto(origin, 0.05);
	ramp rotateto(angles, 0.05);
	wait 0.1;
	logPrint("PROBE slab " + name + " " + ramp.origin + " " + ramp.angles + "\n");
	o = ramp.origin;
	a = ramp.angles;
	logPrint("PROBE r " + name + " " + ((o[0] - origin[0]) * 10000) + " " + ((o[1] - origin[1]) * 10000) + " " + ((o[2] - origin[2]) * 10000) + " " + ((a[0] - angles[0]) * 1000000) + " " + ((a[1] - angles[1]) * 1000000) + " " + ((a[2] - angles[2]) * 1000000) + "\n");
	sample(name, p, ramp, cx, cy, 2);
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
