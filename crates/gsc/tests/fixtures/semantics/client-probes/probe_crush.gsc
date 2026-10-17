//	A blocked mover's slips, mp_carentan only: probe_ride's crush alone, held
//	longer. Keeps the bombzone_A brush model *5 (dm's _gameobjects would
//	delete it), stands the first allied player south of its plank slab, lifts
//	the slab over his head, lowers it onto him and holds it there for 8 s,
//	then teleports him away and sends the slab home with a moveto while it is
//	still descending. Every server frame logs the player's origin and the
//	slab's getorigin():
//
//		PROBE f <phase> <time> <player origin> <mover origin>
//
//	Run by tools/run_probe.sh with a --probe-ride --probe-team allies client,
//	whose RIDE lines carry the commandTime of each snapshot; under a jittered
//	link some frames run with no cmd from him (docs/research/cod11-movers.md
//	12, "A blocked mover").

main()
{
	bms = getentarray("script_brushmodel", "classname");
	for (i = 0; i < bms.size; i++)
		bms[i].script_gameobjectname = "dm";
	thread watch_players();
	maps\mp\gametypes\dm::main();
}

watch_players()
{
	if (getcvar("mapname") != "mp_carentan")
	{
		logPrint("PROBE unsupported " + getcvar("mapname") + "\n");
		return;
	}
	wait 1;
	bms = getentarray("script_brushmodel", "classname");
	bz = undefined;
	for (i = 0; i < bms.size; i++)
		if (!isdefined(bz) || bms[i] getEntityNumber() < bz getEntityNumber())
			bz = bms[i];
	for (;;)
	{
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
			if (players[i] ready())
			{
				players[i] crush(bz);
				return;
			}
		wait 0.05;
	}
}

//	Early returns rather than one compound test: sessionstate is undefined
//	before the first spawn.
ready()
{
	if (!isdefined(self.sessionstate))
		return false;
	if (self.sessionstate != "playing")
		return false;
	if (self.pers["team"] != "allies")
		return false;
	return true;
}

place(spot, yaw)
{
	self setorigin(spot);
	self setplayerangles((0, yaw, 0));
	wait 1;
}

crush(bz)
{
	wait 3;
	logPrint("PROBE player " + self getEntityNumber() + "\n");
	self place((-215, 2430, -31), 90);
	logPrint("PROBE at lift " + getTime() + "\n");
	bz movez(80, 0.5);
	self sample("lift", bz, 1);
	bz movey(-48, 0.5);
	self sample("over", bz, 1);
	logPrint("PROBE at crush " + getTime() + "\n");
	bz movez(-80, 2);
	self sample("crush", bz, 8);
	//	No wait between the teleport and the verb: the slab is still stalled
	//	on the frame the moveto replaces its descent.
	self setorigin((-512, 2688, -16));
	logPrint("PROBE at reset " + getTime() + "\n");
	bz moveto((0, 0, 0), 1);
	self sample("reset", bz, 2);
	logPrint("PROBE done " + getTime() + "\n");
}

sample(phase, ent, secs)
{
	frames = secs * 20;
	for (i = 0; i < frames; i++)
	{
		logPrint("PROBE f " + phase + " " + getTime() + " " + self.origin + " " + ent getorigin() + "\n");
		wait 0.05;
	}
}
