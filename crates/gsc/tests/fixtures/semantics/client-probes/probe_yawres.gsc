//	The residual yaw of a rotateyaw pair on mp_carentan's bombzone_A brush
//	model *5, at full float precision: each frame logs the brush model's
//	.angles (r.currentAngles) scaled by 1e9 and its distance from the
//	rotate's target, through rotateyaw(2, 1), rotateyaw(-2, 1) and a movey.
//
//		PROBE y <phase> <time> <yaw * 1e9> <(yaw - 2) * 1e9> <origin>

main()
{
	bms = getentarray("script_brushmodel", "classname");
	for (i = 0; i < bms.size; i++)
		bms[i].script_gameobjectname = "dm";
	thread run();
	maps\mp\gametypes\dm::main();
}

run()
{
	wait 1;
	bms = getentarray("script_brushmodel", "classname");
	bz = undefined;
	for (i = 0; i < bms.size; i++)
		if (!isdefined(bz) || bms[i] getEntityNumber() < bz getEntityNumber())
			bz = bms[i];
	sample("start", bz, 0.2);
	bz rotateyaw(2, 1);
	sample("yaw", bz, 1.5);
	bz rotateyaw(-2, 1);
	sample("yaw_back", bz, 1.5);
	bz movey(-48, 1);
	sample("move", bz, 1.5);
	bz rotateyaw(10, 1);
	sample("yaw10", bz, 1.5);
	bz rotateyaw(-10, 1);
	sample("yaw10_back", bz, 1.5);
	bz movey(48, 1);
	sample("move2", bz, 1.5);
	logPrint("PROBE done\n");
}

sample(phase, bz, secs)
{
	for (t = 0; t < secs; t += 0.05)
	{
		a = bz.angles;
		logPrint("PROBE y " + phase + " " + getTime() + " " + a[0] * 1000000000 + " " + a[1] * 1000000000 + " " + (a[1] - 2) * 1000000000 + " " + a[2] * 1000000000 + " " + bz getorigin() + "\n");
		wait 0.05;
	}
}
