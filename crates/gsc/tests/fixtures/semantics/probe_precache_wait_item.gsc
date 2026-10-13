// One precache builtin past the script's first wait, after the gametype had
// already precached the same name: a script error on retail
// (docs/research/cod11-gsc-language.md, "Precache builtins").

main()
{
	maps\mp\gametypes\dm::main();
	precacheItem("item_health");
	thread late();
}

late()
{
	wait 0.05;
	logPrint("PROBE at late_item\n");
	precacheItem("item_health");
	logPrint("PROBE late_item ok\n");
}
