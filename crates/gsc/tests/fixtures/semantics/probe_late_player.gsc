// The player-scoped indexers past the first wait on names precached in
// another case: a test client the probe adds (ours connects one itself) sets
// its icons, a shellshock and a viewmodel in its connect callback
// (docs/research/cod11-gsc-language.md 9, "Indexing past the first wait").

main()
{
	maps\mp\gametypes\dm::main();
	level.callbackPlayerConnect = ::onconnect;
	precacheStatusIcon("probe_icon_a");
	precacheHeadIcon("probe_head_a");
	precacheShellShock("default");
	precacheModel("xmodel/crate_misc2a");
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
	self.statusicon = "PROBE_ICON_A";
	logPrint("PROBE statusicon_case " + self.statusicon + "\n");
	self.statusicon = "";
	logPrint("PROBE statusicon_empty " + self.statusicon + "\n");
	self.headicon = "PROBE_HEAD_A";
	logPrint("PROBE headicon_case " + self.headicon + "\n");
	self shellshock("DEFAULT", 1.5);
	logPrint("PROBE shellshock_case ok\n");
	self shellshock("default", 0);
	logPrint("PROBE shellshock_zero ok\n");
	self shellshock("default", 60);
	logPrint("PROBE shellshock_sixty ok\n");
	self stopshellshock();
	logPrint("PROBE stopshellshock ok\n");
	self setViewModel("XMODEL/CRATE_MISC2A");
	logPrint("PROBE viewmodel_case " + self getViewModel() + "\n");
}
