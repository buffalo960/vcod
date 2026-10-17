// attach(model) twice, no tag: both take the empty tag.

main()
{
	maps\mp\gametypes\dm::main();
	ent = spawn("script_model", (0, 0, 0));
	ent attach("xmodel/crate_misc2a");
	logPrint("PROBE at twice_notag\n");
	ent attach("xmodel/crate_misc2a");
	logPrint("PROBE twice_notag ok\n");
}
