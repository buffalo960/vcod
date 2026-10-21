//	United Offensive: the stock jeep self-destruct timer on a jeep that is
//	destroyed and deleted before it runs out (docs/research/
//	bot-objectives.md 10.5). Stock tdm on mp_foy, so `_jeepdrive_gmi` and
//	`_jeepeffects_gmi` run as they do in a game. No client is needed.
//	Two seconds in, the lowest-numbered jeep is marked driven and gets the
//	thread a rider's exit starts, `_jeepdrive_gmi::process_deactivate`,
//	with no deactivator (its 90 s wait shortened by
//	`scr_selfDestructJeepTime`, 20 here unless set). Half a second later the
//	jeep takes the self-destruct's own hit, so its death, respawn and
//	`allow_explode` delete run while the timer waits. Logs, each as
//	`PROBE <step> <time> ...`:
//
//	timer     the jeep's number and the timer's length
//	kill      the hit
//	death     the jeep's `death` notify
//	explode   its `allow_explode` notify
//	gone      the first poll that reads the handle undefined
//	alive     the server still running past the timer's end
//
//	A script error at the timer's `dodamage` ends the run with the
//	console's error block instead of `alive`.
//
//	COD_VERSION=uo SECS=40 tools/run_probe.sh client-probes/probe_jeepdeact
//	mp_foy. UO only.

main()
{
	thread watch();
	maps\mp\gametypes\tdm::main();
}

watch()
{
	wait 2;
	jeeps = getentarray("script_vehicle", "classname");
	jeep = undefined;
	for (i = 0; i < jeeps.size; i++)
	{
		t = jeeps[i].vehicletype;
		if (t != "willyjeep_mp" && t != "horch_mp" && t != "gaz67b_mp")
			continue;
		if (!isdefined(jeep) || jeeps[i] getEntityNumber() < jeep getEntityNumber())
			jeep = jeeps[i];
	}
	if (!isdefined(jeep))
	{
		logPrint("PROBE timer unsupported " + getcvar("mapname") + "\n");
		return;
	}
	secs = getcvarint("scr_selfDestructJeepTime");
	if (secs <= 0)
	{
		setcvar("scr_selfDestructJeepTime", "20");
		secs = 20;
	}
	jeep.has_been_driven = 1;
	logPrint("PROBE timer " + gettime() + " " + jeep getEntityNumber() + " " + jeep.vehicletype + " " + secs + " " + jeep.riding_count + " " + jeep.tank_team + "\n");
	jeep thread maps\mp\_jeepdrive_gmi::process_deactivate(undefined);
	jeep thread log_death();
	wait 0.5;
	logPrint("PROBE kill " + gettime() + " " + jeep.health + "\n");
	jeep dodamage(10000, jeep.origin + (0, 0, 1), jeep, jeep, "MOD_EXPLOSIVE");
	level thread log_gone(jeep);
	wait secs + 2;
	logPrint("PROBE alive " + gettime() + "\n");
}

log_death()
{
	self waittill("death");
	logPrint("PROBE death " + gettime() + "\n");
	self waittill("allow_explode");
	logPrint("PROBE explode " + gettime() + "\n");
}

log_gone(jeep)
{
	while (isdefined(jeep))
		wait 0.05;
	logPrint("PROBE gone " + gettime() + "\n");
}
