// A hud element's setShader past the script's first wait on a name nothing precached: a
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
	logPrint("PROBE at late_setshader\n");
	hud setShader("probe_never", 8, 8);
	logPrint("PROBE late_setshader ok\n");
}
