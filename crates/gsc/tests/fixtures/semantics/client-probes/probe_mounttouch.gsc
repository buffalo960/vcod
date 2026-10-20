//	Whether G_TouchTriggers' area query (mask 0x405c0008) still finds a
//	script trigger_mount after setContents, United Offensive only
//	(docs/research/coduo-movement.md 11). COD_VERSION=uo tools/run_probe.sh
//	client-probes/probe_mounttouch mp_foy, with one spawned
//	`vcod --game-dir <uo install> --mod-dir uo --net-probe <addr>
//	--probe-team allies` standing where he spawned. A standing mount
//	(contents 0x401c4008) goes up around him, then each phase sets its
//	contents and counts the "trigger" notifies over one second:
//
//	PROBE phase <name> old <contents before> fires <n>

main()
{
	thread run();
	maps\mp\gametypes\dm::main();
}

count()
{
	self endon("stop");
	for (;;)
	{
		self waittill("trigger", other);
		self.fires++;
	}
}

phase(name, contents)
{
	old = "-";
	if (isdefined(contents))
		old = self setContents(contents);
	self.fires = 0;
	self thread count();
	wait 1;
	self notify("stop");
	logPrint("PROBE phase " + name + " old " + old + " fires " + self.fires + "\n");
}

run()
{
	for (;;)
	{
		wait 0.5;
		players = getentarray("player", "classname");
		if (players.size > 0 && isdefined(players[0].sessionstate) && players[0].sessionstate == "playing")
			break;
	}
	wait 1;
	o = players[0].origin;
	m = spawnTriggerMount(o + (-40, -40, -10), o + (40, 40, 80), 0);
	logPrint("PROBE mount " + o + "\n");
	m phase("spawned", undefined);
	m phase("standbit", 16384);
	m phase("crouchbit", 4194304);
	m phase("zero", 0);
	m phase("triggerbit", 1073741824);
	m phase("restored", 1075593224);
	logPrint("PROBE done\n");
}
