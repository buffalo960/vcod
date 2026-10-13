// attach past the script's first wait on a name nothing precached: a
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
	logPrint("PROBE at late_attach\n");
	ent attach("xmodel/probe_never");
	logPrint("PROBE late_attach ok\n");
}
