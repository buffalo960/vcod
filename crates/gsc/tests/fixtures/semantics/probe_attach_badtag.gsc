// getAttachTagName at the attachment count.

main()
{
	maps\mp\gametypes\dm::main();
	ent = spawn("script_model", (0, 0, 0));
	ent attach("xmodel/crate_misc2a", "tag_a");
	logPrint("PROBE at badtag\n");
	m = ent getAttachTagName(1);
	logPrint("PROBE badtag ok\n");
}
