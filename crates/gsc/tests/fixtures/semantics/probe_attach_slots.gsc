// attach's six slots (game.mp.i386.so G_EntAttach 0x66190): detach closes
// the gap behind it, the tag reads back lower case, and a seventh model is a
// script error
// (docs/research/cod11-gsc-object-model.md, "attach and detach").

main()
{
	maps\mp\gametypes\dm::main();
	ent = spawn("script_model", (0, 0, 0));
	ent attach("xmodel/crate_misc2a", "TAG_A");
	ent attach("xmodel/crate_misc2a", "tag_b");
	ent attach("xmodel/crate_misc2a");
	ent attach("XMODEL/CRATE_MISC2A", "tag_d");
	logPrint("PROBE size4 " + ent getAttachSize() + "\n");
	logPrint("PROBE tag0 " + ent getAttachTagName(0) + "\n");
	logPrint("PROBE tag2 [" + ent getAttachTagName(2) + "]\n");
	logPrint("PROBE model3 " + ent getAttachModelName(3) + "\n");
	ent detach("xmodel/crate_misc2a", "Tag_B");
	logPrint("PROBE size_detached " + ent getAttachSize() + "\n");
	for(i = 0; i < ent getAttachSize(); i++)
		logPrint("PROBE after_detach" + i + " " + ent getAttachModelName(i) + " tag [" + ent getAttachTagName(i) + "]\n");
	ent detach("xmodel/crate_misc2a");
	logPrint("PROBE size_notag " + ent getAttachSize() + " tag1 " + ent getAttachTagName(1) + "\n");
	ent attach("xmodel/crate_misc2a");
	ent attach("xmodel/crate_misc2a", "tag_e");
	logPrint("PROBE size_filled " + ent getAttachSize() + "\n");
	logPrint("PROBE tag3 " + ent getAttachTagName(3) + "\n");
	ent attach("xmodel/crate_misc2a", "tag_f");
	ent attach("xmodel/crate_misc2a", "tag_g");
	logPrint("PROBE size6 " + ent getAttachSize() + "\n");
	ent detachAll();
	logPrint("PROBE size_cleared " + ent getAttachSize() + "\n");
	for(i = 0; i < 6; i++)
		ent attach("xmodel/crate_misc2a", "tag_" + i);
	logPrint("PROBE at seventh\n");
	ent attach("xmodel/crate_misc2a", "tag_6");
	logPrint("PROBE seventh ok\n");
}
