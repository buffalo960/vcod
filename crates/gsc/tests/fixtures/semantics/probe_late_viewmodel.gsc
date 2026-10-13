// setViewModel on a player, here a test client the probe adds past the first wait
// (ours connects one itself), on a name nothing precached: a script error on
// retail (docs/research/cod11-gsc-language.md 9, "Indexing past the first
// wait").

main()
{
	maps\mp\gametypes\dm::main();
	level.callbackPlayerConnect = ::onconnect;
	precacheShellShock("default");
	thread late();
}

// Retail refuses a test client within sv_reconnectlimit (3 s) of the
// server's start: "reconnect rejected : too soon".
late()
{
	wait 5;
	if(!isDefined(level.probe_player))
		addtestclient();
}

onconnect()
{
	level.probe_player = self;
	logPrint("PROBE at late_viewmodel\n");
	self setViewModel("xmodel/probe_never");
	logPrint("PROBE late_viewmodel ok\n");
}
