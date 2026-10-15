// What `G_GetNonPVSTankInfo` packs into a United Offensive client's
// `iCompassTankInfo`. UO only. The tank (the lowest-numbered
// `script_vehicle` of the `probe_vehicletype` cvar's type) goes on the
// compass with `addVehicleToCompass`, which no stock MP script calls.
// Client 0 is set down beside it and drives it (`--probe-vehicle driver`,
// start it first); client 1, its teammate (`--probe-compass`, same
// `--probe-team`), is moved through the tdm spawns once the driver sits.
// Each `PROBE step` line says where the viewer and the tank are; the
// viewer's `COMPASS` lines are the measurement. Then the tank leaves the
// compass, comes back, the viewer goes round six spawns again while the
// tank drives, and changes team. Recipe: `COD_VERSION=uo SECS=90
// tools/run_probe.sh client-probes/probe_tankcompass mp_cassino +set
// probe_vehicletype panzeriv_mp`, then `vcod --game-dir <UO install>
// --mod-dir uo --net-probe <addr> --probe-team axis --probe-vehicle driver
// --probe-secs 80` and, 3 s later, the same with `--probe-compass` in
// place of `--probe-vehicle driver`.
main()
{
	thread place();
	thread wander();
	maps\mp\gametypes\tdm::main();
}

probe_vehicle()
{
	want = getcvar("probe_vehicletype");
	vehs = getentarray("script_vehicle", "classname");
	best = undefined;
	for (i = 0; i < vehs.size; i++)
	{
		if (want != "" && vehs[i].vehicletype != want)
			continue;
		if (!isdefined(best) || vehs[i] getEntityNumber() < best getEntityNumber())
			best = vehs[i];
	}
	return best;
}

client(n)
{
	players = getentarray("player", "classname");
	for (i = 0; i < players.size; i++)
		if (players[i] getEntityNumber() == n)
			return players[i];
	return undefined;
}

//	Client 0 beside the tank facing it, as probe_vehicle's try_place.
place()
{
	wait 2;
	veh = probe_vehicle();
	if (!isdefined(veh))
	{
		logPrint("PROBE place unsupported " + getcvar("mapname") + "\n");
		return;
	}
	level.probe_veh = veh;
	veh addVehicleToCompass();
	logPrint("PROBE vehicle " + veh getEntityNumber() + " " + veh.vehicletype + " " + veh.origin + " " + veh.angles + "\n");
	placed = 0;
	while (!placed)
	{
		wait 0.05;
		p = client(0);
		if (!isdefined(p) || !isdefined(p.sessionstate) || p.sessionstate != "playing")
			continue;
		right = anglestoright(veh.angles);
		spot = veh.origin + (right[0] * 110, right[1] * 110, 8);
		p setorigin(spot);
		p setplayerangles((10, vectortoangles(veh.origin - spot)[1], 0));
		logPrint("PROBE place " + gettime() + " 0 " + spot + "\n");
		placed = 1;
	}
}

step(viewer, name)
{
	veh = level.probe_veh;
	logPrint("PROBE step " + gettime() + " " + name + " viewer " + viewer.origin + " tank " + veh.origin + " " + veh.angles + "\n");
	wait 1.5;
}

wander()
{
	for (;;)
	{
		wait 0.05;
		d = client(0);
		v = client(1);
		if (isdefined(d) && isdefined(v) && isdefined(level.probe_veh) && d isInVehicle()
			&& isdefined(v.sessionstate) && v.sessionstate == "playing")
			break;
	}
	//	tdm's team balance puts the second joiner on the other side.
	v.sessionteam = d.sessionteam;
	wait 1;
	spawns = getentarray("mp_teamdeathmatch_spawn", "classname");
	for (i = 0; i < spawns.size && i < 16; i++)
	{
		v = client(1);
		v setorigin(spawns[i].origin);
		v setplayerangles(spawns[i].angles);
		step(v, "spawn" + i);
	}
	v = client(1);
	level.probe_veh removeVehicleFromCompass();
	step(v, "removed");
	level.probe_veh addVehicleToCompass();
	step(v, "added");
	//	Again while the tank drives.
	for (i = 0; i < spawns.size && i < 6; i++)
	{
		v setorigin(spawns[i].origin);
		v setplayerangles(spawns[i].angles);
		step(v, "again" + i);
	}
	if (v.sessionteam == "axis")
		v.sessionteam = "allies";
	else
		v.sessionteam = "axis";
	step(v, "other_team");
	logPrint("PROBE done\n");
}
