// getAttachModelName at the attachment count: a script error, not undefined.

main()
{
	maps\mp\gametypes\dm::main();
	ent = spawn("script_model", (0, 0, 0));
	ent attach("xmodel/crate_misc2a", "tag_a");
	logPrint("PROBE at badindex\n");
	m = ent getAttachModelName(1);
	logPrint("PROBE badindex ok\n");
}
