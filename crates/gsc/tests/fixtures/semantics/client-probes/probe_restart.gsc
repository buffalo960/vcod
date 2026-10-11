//	When the first snapshot after a map_restart reaches a client that was
//	in the world. dm, plus a thread that waits for a player, waits
//	`probe_wait` seconds (default 3), logs the frame and calls
//	map_restart(probe_persist), `probe_count` times (default 3; the count
//	rides a cvar, which outlives both restarts). `probe_play 1` puts the
//	client in play at a fixed spot rather than leaving it a spectator.
//	Logs:
//
//		PROBE restart <n> <time> <persist>
//		PROBE start <time>
//
//	Run by tools/run_probe.sh with one `--net-probe --probe-edge watch`
//	client; its `EDGE snap` lines around each restart frame are the
//	measurement (docs/research/cod11-map-cycle.md, 4.7).

main()
{
	maps\mp\gametypes\dm::main();
	level.probe_dm_connect = level.callbackPlayerConnect;
	level.callbackPlayerConnect = ::Callback_PlayerConnect;
}

Callback_PlayerConnect()
{
	if (getcvar("probe_play") == "1")
	{
		self waittill("begin");
		self.sessionteam = "allies";
		self.sessionstate = "playing";
		self.maxhealth = 100;
		self.health = 100;
		self spawn((976, -864, 0), (0, 90, 0));
	}
	else
		self thread [[level.probe_dm_connect]]();
	if (!isdefined(level.probe_armed))
	{
		level.probe_armed = 1;
		level thread restart_later();
	}
}

restart_later()
{
	logPrint("PROBE start " + getTime() + "\n");
	done = getcvarint("probe_done");
	count = 3;
	if (getcvar("probe_count") != "")
		count = getcvarint("probe_count");
	if (done >= count)
		return;
	w = 3;
	if (getcvar("probe_wait") != "")
		w = getcvarint("probe_wait");
	wait w;
	setcvar("probe_done", done + 1);
	persist = getcvarint("probe_persist");
	logPrint("PROBE restart " + (done + 1) + " " + getTime() + " " + persist + "\n");
	map_restart(persist);
}
