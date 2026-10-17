// detach of a pair the entity does not carry: a script error after the
// console's attachment list (game.mp.i386.so 0x59c33).

main()
{
	maps\mp\gametypes\dm::main();
	ent = spawn("script_model", (0, 0, 0));
	ent attach("xmodel/crate_misc2a", "tag_a");
	logPrint("PROBE at detach_missing\n");
	ent detach("xmodel/crate_misc2a", "tag_b");
	logPrint("PROBE detach_missing ok\n");
}
