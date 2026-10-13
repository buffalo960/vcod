// objective_add's icon past the script's first wait on a name nothing precached: a
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
	logPrint("PROBE at late_objective\n");
	objective_add(0, "current", (0, 0, 0), "probe_never");
	logPrint("PROBE late_objective ok\n");
}
