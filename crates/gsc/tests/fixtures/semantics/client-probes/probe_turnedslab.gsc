//	A turned submodel against G_MoverPush's box tests, for
//	docs/research/cod11-movers.md 22. mp_harbor only. The bel gametype's
//	script_brushmodel *9 is one brush, a slab 4 thick along x, 56 along y,
//	96 tall, its entity at the world origin. The probe keeps it from
//	_gameobjects and in one frame turns it 45 degrees and moves it so its
//	centre B lands on C, an open floor at z 0 (origin to O = C - R B, yaw to
//	45, its bottom at z 1), then stands the player at d units from C along
//	the slab's turned +x face normal. Each stand lifts the slab 2 units
//	over 0.1 s and lowers it back. A player box axial in the slab's frame
//	clears the face past d = 17; one axial in the world's reaches it to
//	d = 23.2. Every server frame of a stand logs
//
//		PROBE f <d> <time> <player origin> <slab origin> <slab angles>
//		PROBE g <d> <time> <player x - C.x> <player y - C.y> <player z> <slab z>
//
//	the second's floats printed by the engine's %g, six digits.
//	Run by tools/run_probe.sh client-probes/probe_turnedslab mp_harbor with a
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
	// The slab's centre as built, and where the turn puts it: an open
	// floor at z 0.
	bx = -9638;
	by = -8003;
	cx = -11000;
	cy = -7656;
	// cos 45 and sin 45.
	c = 0.707107;
	s = 0.707107;
	o = (cx - (bx * c - by * s), cy - (bx * s + by * c), -7);
	slab moveto(o, 0.05);
	slab rotateyaw(45, 0.05);
	wait 0.5;
	logPrint("PROBE slab " + slab getEntityNumber() + " " + slab.origin + " " + slab.angles + "\n");

	d = [];
	d[0] = 25;
	d[1] = 22;
	d[2] = 20;
	d[3] = 18;
	for (k = 0; k < d.size; k++)
	{
		p setorigin((cx + c * d[k], cy + s * d[k], 1));
		p setplayerangles((0, 225, 0));
		wait 1;
		slab movez(2, 0.1);
		sample(d[k], p, slab, cx, cy, 0.5);
		slab movez(-2, 0.1);
		sample(d[k], p, slab, cx, cy, 0.5);
	}
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

sample(d, p, e, cx, cy, secs)
{
	frames = secs * 20;
	for (i = 0; i < frames; i++)
	{
		logPrint("PROBE f " + d + " " + getTime() + " " + p.origin + " " + e.origin + " " + e.angles + "\n");
		logPrint("PROBE g " + d + " " + getTime() + " " + (p.origin[0] - cx) + " " + (p.origin[1] - cy) + " " + p.origin[2] + " " + e.origin[2] + "\n");
		wait 0.05;
	}
}
