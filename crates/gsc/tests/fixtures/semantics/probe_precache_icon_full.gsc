// A ninth status icon (docs/research/cod11-gsc-language.md, "Precache
// builtins"): main takes seven, then dm's Callback_StartGameType precaches
// its two and the second of them is the script error.

main()
{
	maps\mp\gametypes\dm::main();
	precacheStatusIcon("probe_icon_1");
	precacheStatusIcon("probe_icon_2");
	precacheStatusIcon("probe_icon_3");
	precacheStatusIcon("probe_icon_4");
	precacheStatusIcon("probe_icon_5");
	precacheStatusIcon("probe_icon_6");
	logPrint("PROBE at icon_seventh\n");
	precacheStatusIcon("probe_icon_7");
	logPrint("PROBE icon_seventh ok\n");
}
