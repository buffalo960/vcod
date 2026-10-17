//	Player movement against patches. Under probe_teleport 1 it puts each
//	spawning player at a spot on mp_carentan beside a lump 24 patch, facing
//	it, once per spawn, for --save-slope --probe-patchwalk <yaw>. probe_spot
//	pole is patch 4 (yaw 45), pillar patch 11 (yaw 0, terrain floor), kerb
//	patch 98 (yaw 180, a run of small patches either side).
//	docs/research/bsp-ibsp59-format.md, "Patch collision on 1.5 and UO".

main()
{
	thread watch_teleports();
	maps\mp\gametypes\dm::main();
}

watch_teleports()
{
	if (getcvar("probe_teleport") != "1")
		return;
	wait 1;
	for (;;)
	{
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
			players[i] try_place();
		wait 0.05;
	}
}

//	Early returns rather than one compound test: sessionstate is undefined
//	before the first spawn.
try_place()
{
	if (!isdefined(self.sessionstate))
		return;
	if (self.sessionstate != "playing")
	{
		self.probe_placed = undefined;
		return;
	}
	if (isdefined(self.probe_placed))
		return;
	self.probe_placed = 1;
	spot = getcvar("probe_spot");
	if (spot == "pillar")
	{
		origin = (544, 768, -9.718);
		yaw = 0;
	}
	else if (spot == "kerb")
	{
		origin = (1408, 1504, -23.748);
		yaw = 180;
	}
	else
	{
		origin = (-160, 2304, -31.748);
		yaw = 45;
	}
	self setorigin(origin);
	self setplayerangles((0, yaw, 0));
	logPrint("PROBE place " + getTime() + " " + self getEntityNumber() + " " + origin + " " + yaw + "\n");
}
