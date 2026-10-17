//	SV_LinkEntity's angle snap on entities that are not movers, mp_carentan,
//	no client. Each kind (a script_model, a script_origin, a suspended
//	health pack, bombzone_A's brush model *5) gets two angle sets written
//	through .angles, then an .origin write, which links, then a frame's
//	wait. Every step logs .angles (r.currentAngles) times 1e9:
//
//		PROBE a <kind> <set> <step> <pitch * 1e9> <yaw * 1e9> <roll * 1e9>
//
//	<step> is `set` (after the .angles write), `linked` (after the .origin
//	write) and `later` (a frame on).

main()
{
	bms = getentarray("script_brushmodel", "classname");
	for (i = 0; i < bms.size; i++)
		bms[i].script_gameobjectname = "dm";
	precacheModel("xmodel/health_large");
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
	m = spawn("script_model", (-400, 2600, 40));
	m setmodel("xmodel/health_large");
	o = spawn("script_origin", (-420, 2600, 40));
	w = spawn("item_health", (-440, 2600, 40), 1);
	ents = [];
	ents[0] = m;
	ents[1] = o;
	ents[2] = w;
	ents[3] = bz;
	kinds = [];
	kinds[0] = "model";
	kinds[1] = "origin";
	kinds[2] = "item";
	kinds[3] = "brush";
	sets = [];
	sets[0] = (0.0004, 89.9996, 0.0011);
	sets[1] = (-0.9995, 180.0008, 359.9991);
	for (s = 0; s < sets.size; s++)
	{
		for (i = 0; i < ents.size; i++)
		{
			ents[i].angles = sets[s];
			show(kinds[i], s, "set", ents[i]);
			ents[i].origin = ents[i].origin + (0, 0, 1);
			show(kinds[i], s, "linked", ents[i]);
		}
		wait 0.05;
		for (i = 0; i < ents.size; i++)
			show(kinds[i], s, "later", ents[i]);
	}
	logPrint("PROBE done\n");
}

show(kind, set, step, ent)
{
	a = ent.angles;
	logPrint("PROBE a " + kind + " " + set + " " + step + " " + a[0] * 1000000000 + " " + a[1] * 1000000000 + " " + a[2] * 1000000000 + "\n");
}
