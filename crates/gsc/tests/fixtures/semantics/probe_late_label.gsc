// A hud element's label past the script's first wait on a name nothing precached: a
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
	hud = newHudElem();
	logPrint("PROBE at late_label\n");
	hud.label = &"PROBE_NEVER";
	logPrint("PROBE late_label ok\n");
}
