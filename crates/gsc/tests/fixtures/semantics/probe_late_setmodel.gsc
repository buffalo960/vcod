// setModel past the script's first wait on a name nothing precached: a
// script error on retail (docs/research/cod11-gsc-language.md 9, "Indexing
// past the first wait").

main()
{
	maps\mp\gametypes\dm::main();
	thread late();
}

late()
{
	wait 0.05;
	ent = spawn("script_model", (0, 0, 0));
	logPrint("PROBE at late_setmodel\n");
	ent setModel("xmodel/probe_never");
	logPrint("PROBE late_setmodel ok\n");
}
