//	Whether 1.5's mp_neuville clips its lump-24 patches no leaf lists
//	(bsp-ibsp59-format.md, "The leaf walk"): patches 68 and 69 are upright
//	arches in the plane x = -15480 with open air in front of them, so a
//	bulletTrace run west through one stops on it at x -15479.875 if it
//	clips and on the wall behind at x -15483.875 if not. The other four
//	(65, 66, 71, 72) lie inside a brush and no trace can tell. Run by
//	tools/run_probe.sh with COD_VERSION=1.5 on mp_neuville; no client
//	needed. The lines are crates/common/tests/fixtures/patch/neuvilleleaf-15.txt's.

main()
{
	maps\mp\gametypes\dm::main();
	thread probe();
}

probe()
{
	wait 1;
	spots = [];
	spots[spots.size] = (-15440, 4871, 92.5);
	spots[spots.size] = (-15440, 4870, 93);
	spots[spots.size] = (-15440, 4869, 93.5);
	spots[spots.size] = (-15440, 4825, 92.5);
	spots[spots.size] = (-15440, 4826, 93);
	spots[spots.size] = (-15440, 4827, 93.5);
	for (i = 0; i < spots.size; i++)
	{
		s = spots[i];
		t = bulletTrace(s, s - (80, 0, 0), false, undefined);
		logPrint("PROBE point " + i + " " + s + " " + t["fraction"] + " " + t["position"] + "\n");
	}
	logPrint("PROBE done\n");
}
