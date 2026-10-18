//	Whether a terrain partition the leaves list clips a trace that reaches
//	it from outside the world (bsp-ibsp59-format.md, "The leaf walk"): a
//	bulletTrace and a falling weapon item straight down 400 units from
//	spots 8 units over mp_powcamp's terrain past its playable walls, where
//	the BSP walk enters only solid leaves, which list no partition. The
//	bulletTrace is a point, the item's (-1,-1,-1)..(1,1,1) sweep a
//	capsule. Run by tools/run_probe.sh on mp_powcamp (1.1d); no client
//	needed. The lines are crates/common/tests/fixtures/patch/outterrain.txt's.

main()
{
	maps\mp\gametypes\dm::main();
	precacheItem("kar98k_mp");
	thread probe();
}

probe()
{
	wait 1;
	spots = [];
	spots[spots.size] = (2621.4, -1478.0, 93.5);
	spots[spots.size] = (3019.1, 3012.3, 205.2);
	spots[spots.size] = (1248.4, 6964.5, 197.5);
	spots[spots.size] = (3755.4, 2968.9, 161.3);
	spots[spots.size] = (2306.4, 7020.1, 262.4);
	spots[spots.size] = (1827, 6983, 206);
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
