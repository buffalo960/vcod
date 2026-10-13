// The precache builtins around the script's first wait
// (docs/research/cod11-gsc-language.md, "Precache builtins"). While the level
// initializes each one answers nothing; menus and icons fold case when they
// meet a name twice. Past the first wait the menu and icon builtins still
// allocate, and precacheModel refuses even a model already loaded.

main()
{
	maps\mp\gametypes\dm::main();
	logPrint("PROBE ret_model " + isDefined(precacheModel("xmodel/crate_misc2a")) + "\n");
	logPrint("PROBE ret_shader " + isDefined(precacheShader("white")) + "\n");
	logPrint("PROBE ret_string " + isDefined(precacheString(&"MPSCRIPT_KILLCAM")) + "\n");
	logPrint("PROBE ret_item " + isDefined(precacheItem("item_health")) + "\n");
	logPrint("PROBE ret_shellshock " + isDefined(precacheShellShock("default")) + "\n");
	logPrint("PROBE ret_menu " + isDefined(precacheMenu("probe_menu_a")) + "\n");
	logPrint("PROBE ret_statusicon " + isDefined(precacheStatusIcon("probe_icon_1")) + "\n");
	logPrint("PROBE ret_headicon " + isDefined(precacheHeadIcon("probe_head_1")) + "\n");
	// Six status icons here and dm's two in Callback_StartGameType fill all
	// eight, so a seventh here in another case dies there unless it is the
	// same name.
	precacheStatusIcon("probe_icon_2");
	precacheStatusIcon("probe_icon_3");
	precacheStatusIcon("probe_icon_4");
	precacheStatusIcon("probe_icon_5");
	precacheStatusIcon("probe_icon_6");
	logPrint("PROBE at icon_case\n");
	precacheStatusIcon("PROBE_ICON_1");
	logPrint("PROBE icon_case ok\n");
	thread late();
}

late()
{
	wait 0.05;
	precacheMenu("probe_menu_late");
	logPrint("PROBE late_menu ok\n");
	precacheHeadIcon("probe_head_late");
	logPrint("PROBE late_headicon ok\n");
	precacheStatusIcon("probe_icon_2");
	logPrint("PROBE late_statusicon ok\n");
	logPrint("PROBE at late_model\n");
	precacheModel("xmodel/crate_misc2a");
	logPrint("PROBE late_model ok\n");
}
