//	What a trace clips of lump 24 (bsp-ibsp59-format.md, "The leaf walk"):
//	a bulletTrace and a falling weapon item straight down 400 units from
//	each spot. The bulletTrace is a point, the item's (-1,-1,-1)..(1,1,1)
//	sweep a capsule. Each spot sits over a patch the leaves under it do not
//	list (mp_railyard, mp_depot, mp_harbor) or one a submodel owns (mp_rocket's
//	*4, which dm keeps, and mp_ship's dock, which mp_ship.gsc deletes). Run by
//	tools/run_probe.sh on each map named below; no client needed. The lines
//	are crates/common/tests/fixtures/patch/leafgate.txt's.

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
	if (map == "mp_railyard")
	{
		spots[spots.size] = (2887, -2120, 967);
		spots[spots.size] = (2823, -2096, 967);
		spots[spots.size] = (2839, -2080, 967);
	}
	else if (map == "mp_depot")
	{
		spots[spots.size] = (-502, 2061, 115);
		spots[spots.size] = (-502, 2069, 115);
		spots[spots.size] = (-494, 2077, 115);
	}
	else if (map == "mp_rocket")
	{
		spots[spots.size] = (10671, 5825, 183);
		spots[spots.size] = (10655, 5825, 183);
		spots[spots.size] = (12428, 5066, 393);
	}
	else if (map == "mp_ship")
	{
		spots[spots.size] = (7710, -568, 4);
		spots[spots.size] = (7702, -552, 5);
		spots[spots.size] = (2922, -119, 184);
	}
	else if (map == "mp_harbor")
	{
		spots[spots.size] = (-9661, -8921, 373);
	}
	ents = getentarray("script_brushmodel", "classname");
	for (i = 0; i < ents.size; i++)
		logPrint("PROBE brushmodel " + ents[i] getEntityNumber() + " tn " + str(ents[i].targetname) + " go " + str(ents[i].script_gameobjectname) + " ex " + str(ents[i].script_exploder) + "\n");
	items = [];
	for (i = 0; i < spots.size; i++)
	{
		s = spots[i];
		t = bulletTrace(s, s - (0, 0, 400), false, undefined);
		logPrint("PROBE point " + i + " " + t["fraction"] + " " + t["position"] + "\n");
		if (isdefined(t["entity"]))
			logPrint("PROBE entity " + i + " " + t["entity"] getEntityNumber() + " " + t["entity"].classname + " " + t["entity"].model + "\n");
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

str(v)
{
	if (!isdefined(v))
		return "undefined";
	return v;
}
