//	Whether a lump-24 terrain partition no leaf lists clips (bsp-ibsp59-format.md,
//	"The leaf walk"): a bulletTrace and a falling weapon item straight down
//	400 units from spots over unlisted terrain on United Offensive's mp_kursk
//	and mp_peaks, where ours falls past the partition's plane. The bulletTrace
//	is a point, the item's (-1,-1,-1)..(1,1,1) sweep a capsule. Run by
//	tools/run_probe.sh with COD_VERSION=uo on each map named below; no client
//	needed. The lines are crates/common/tests/fixtures/patch/terrainleaf-uo.txt's.

main()
{
	maps\mp\gametypes\dm::main();
	precacheItem("kar98k_mp");
	thread probe();
}

probe()
{
	wait 1;
	map = getcvar("mapname");
	spots = [];
	if (map == "mp_kursk")
	{
		spots[spots.size] = (90261, 1813, 72);
		spots[spots.size] = (98027, 2027, 712);
		spots[spots.size] = (92053, 4203, 200);
		spots[spots.size] = (92125, 3077, 200);
	}
	else if (map == "mp_peaks")
	{
		spots[spots.size] = (-5678, 1985, 84);
		spots[spots.size] = (-5689, 2062, 84);
	}
	items = [];
	for (i = 0; i < spots.size; i++)
	{
		s = spots[i];
		t = bulletTrace(s, s - (0, 0, 400), false, undefined);
		logPrint("PROBE point " + i + " " + s + " " + t["fraction"] + " " + t["position"] + "\n");
		items[i] = spawn("mpweapon_kar98k", s);
	}
	wait 3;
	for (i = 0; i < items.size; i++)
	{
		if (isdefined(items[i]))
			logPrint("PROBE item " + i + " " + items[i].origin + "\n");
		else
			logPrint("PROBE item " + i + " gone\n");
	}
	logPrint("PROBE done\n");
}
