// The same model on the same tag twice: attach detaches the first and
// errors (game.mp.i386.so 0x5d39c). The model compares without case, the tag
// through its lower-case script string.

main()
{
	maps\mp\gametypes\dm::main();
	ent = spawn("script_model", (0, 0, 0));
	ent attach("xmodel/crate_misc2a", "tag_a");
	logPrint("PROBE at twice\n");
	ent attach("XMODEL/crate_misc2a", "TAG_A");
	logPrint("PROBE twice ok\n");
}
