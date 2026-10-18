//	Whether a patch facet stops a trace from behind its surface
//	(bsp-ibsp59-format.md, "Patch collision"): United Offensive's mp_streets
//	patch 391 is a flat corner of the arch over the window at (292, -6660),
//	in the plane x = 292 facing +x, y -6657 to -6633 and z 173 to 201. A
//	bulletTrace 40 units along x through it from the front (x 312 to 272)
//	and from behind (272 to 312) at three spots on the facets. Run by
//	tools/run_probe.sh with COD_VERSION=uo on mp_streets; no client needed.
//	The lines are crates/common/tests/fixtures/patch/patchside-uo.txt's.

main()
{
	maps\mp\gametypes\dm::main();
	thread probe();
}

probe()
{
	wait 1;
	spots = [];
	spots[spots.size] = (0, -6650, 199.5);
	spots[spots.size] = (0, -6640, 195);
	spots[spots.size] = (0, -6634.5, 180);
	for (i = 0; i < spots.size; i++)
	{
		s = spots[i];
		front = (312, s[1], s[2]);
		back = (272, s[1], s[2]);
		t = bulletTrace(front, back, false, undefined);
		logPrint("PROBE front " + i + " " + front + " " + t["fraction"] + " " + t["position"] + " " + t["normal"] + "\n");
		t = bulletTrace(back, front, false, undefined);
		logPrint("PROBE back " + i + " " + back + " " + t["fraction"] + " " + t["position"] + " " + t["normal"] + "\n");
	}
	logPrint("PROBE done\n");
}
