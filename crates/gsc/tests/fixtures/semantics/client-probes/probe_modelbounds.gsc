//	A script model's whole-model bounds in a locational trace's way
//	(docs/research/cod11-combat.md 2.7). Stock tdm, any patch, no client.
//
//	`SV_ClipMoveToEntity` rejects an `svFlags` 4 model before it clips a
//	single collision triangle when the segment misses the xmodel's own
//	bounds, unrotated, about its origin. Two models whose collision pokes
//	out of those bounds are spawned in turn 200 above the first team
//	deathmatch spawn point: `xmodel/vehicle_car` (collision x -96..111,
//	bounds a cube of 94.37) and `xmodel/cow_dead2` (bounds all 0). Each is
//	turned to every yaw in its list, and at each yaw a level `bulletTrace`
//	(hitCharacters) is run across the model along its right vector, 150
//	either side of it, at every offset `a` along its forward and height
//	`h` above its origin in the lists:
//
//	PROBE model <name> <entity number> <origin>
//	PROBE btrace <name> <yaw> <a> <h> <entity number or -1> <fraction>

main()
{
	precacheModel("xmodel/vehicle_car");
	precacheModel("xmodel/cow_dead2");
	thread run();
	maps\mp\gametypes\tdm::main();
}

entnum(e)
{
	if (!isdefined(e))
		return -1;
	return e getEntityNumber();
}

sweep(name, at, yaws, offsets, heights)
{
	model = spawn("script_model", at);
	model setmodel(name);
	logPrint("PROBE model " + name + " " + model getEntityNumber() + " " + model.origin + "\n");
	for (y = 0; y < yaws.size; y++)
	{
		model.angles = (0, yaws[y], 0);
		wait 0.1;
		fwd = anglestoforward((0, yaws[y], 0));
		right = anglestoright((0, yaws[y], 0));
		for (i = 0; i < offsets.size; i++)
			for (j = 0; j < heights.size; j++)
			{
				mid = at + (fwd[0] * offsets[i], fwd[1] * offsets[i], heights[j]);
				from = mid - (right[0] * 150, right[1] * 150, 0);
				to = mid + (right[0] * 150, right[1] * 150, 0);
				trace = bullettrace(from, to, true, undefined);
				logPrint("PROBE btrace " + name + " " + yaws[y] + " " + offsets[i] + " " + heights[j] + " " + entnum(trace["entity"]) + " " + trace["fraction"] + "\n");
			}
	}
	model delete();
	wait 0.1;
}

run()
{
	wait 2;
	spawns = getentarray("mp_teamdeathmatch_spawn", "classname");
	if (!spawns.size)
		spawns = getentarray("mp_tdm_spawn", "classname");
	at = spawns[0].origin + (0, 0, 200);

	yaws = [];
	yaws[0] = 0;
	yaws[1] = 45;
	yaws[2] = 90;
	yaws[3] = 180;
	offsets = [];
	offsets[0] = -95;
	offsets[1] = -90;
	offsets[2] = 60;
	offsets[3] = 90;
	offsets[4] = 94;
	offsets[5] = 95;
	offsets[6] = 98;
	offsets[7] = 102;
	offsets[8] = 106;
	offsets[9] = 110;
	heights = [];
	heights[0] = 15;
	heights[1] = 30;
	heights[2] = 45;
	sweep("xmodel/vehicle_car", at, yaws, offsets, heights);

	yaws = [];
	yaws[0] = 0;
	yaws[1] = 90;
	offsets = [];
	offsets[0] = -20;
	offsets[1] = 0;
	offsets[2] = 20;
	heights = [];
	heights[0] = 5;
	heights[1] = 15;
	heights[2] = 25;
	sweep("xmodel/cow_dead2", at, yaws, offsets, heights);
	logPrint("PROBE done\n");
}
