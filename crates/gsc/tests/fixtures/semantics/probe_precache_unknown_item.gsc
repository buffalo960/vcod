// An item name bg_itemlist lacks (docs/research/cod11-gsc-language.md,
// "Precache builtins").

main()
{
	maps\mp\gametypes\dm::main();
	logPrint("PROBE at unknown_item\n");
	precacheItem("probe_no_such_item");
	logPrint("PROBE unknown_item ok\n");
}
