//	G_RunFrame's trigger queue when it fills (docs/research/cod11-gsc-language.md,
//	"Engine notifies and the trigger queue"). No client: stock tdm under
//	tools/run_probe.sh on mp_carentan. One script-spawned trigger_damage,
//	opened by enableLinkTo so no wait is armed and every blast activates it,
//	takes radiusDamage at its box centre (probe_dmgspawn's) N times in one
//	script step. Each activation queues a "trigger"; the 257th finds the
//	queue's 0x100 entries full and notifies on the spot.
//
//	PROBE phase <time> <name>
//	PROBE trigger <time> <activator number>

main()
{
	thread run();
	maps\mp\gametypes\tdm::main();
}

watch_trigger()
{
	self endon("death");
	for (;;)
	{
		self waittill("trigger", other);
		logPrint("PROBE trigger " + gettime() + " " + other getEntityNumber() + "\n");
	}
}

blasts(name, at, n)
{
	logPrint("PROBE phase " + gettime() + " " + name + "\n");
	for (i = 0; i < n; i++)
		radiusDamage(at, 64, 100, 50);
	wait 1;
}

run()
{
	wait 1;
	o = (0, 0, 0);
	sp = getentarray("mp_teamdeathmatch_spawn", "classname");
	if (sp.size > 0)
		o = sp[0].origin;
	at = o + (1152, 2464, 865.93863);
	t = spawn("trigger_damage", o);
	t enableLinkTo();
	t thread watch_trigger();
	wait 0.5;
	blasts("one", at, 1);
	blasts("full", at, 256);
	blasts("over", at, 257);
	blasts("over3", at, 259);
	// Off the frame grid: on 1.1 this thread runs in G_RunFrame's second
	// thread pass, after the queue was handed out.
	wait 0.06;
	blasts("offgrid", at, 1);
	wait 0.06;
	blasts("offgrid_over", at, 258);
	logPrint("PROBE done\n");
}
