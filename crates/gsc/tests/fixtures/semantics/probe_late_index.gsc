// The builtins that index a configstring at run time, past the script's
// first wait, on names the gametype precached in another case
// (docs/research/cod11-gsc-language.md 9, "Indexing past the first wait").
// Each finds the name and reads back the precached spelling; a new name is
// a script error, one probe_late_* file each.

main()
{
	maps\mp\gametypes\dm::main();
	precacheModel("xmodel/crate_misc2a");
	precacheShader("white");
	thread late();
}

late()
{
	wait 0.05;
	ent = spawn("script_model", (0, 0, 0));
	ent setModel("XMODEL/CRATE_MISC2A");
	logPrint("PROBE setmodel_case " + ent.model + "\n");
	ent setModel("");
	logPrint("PROBE setmodel_empty " + ent.model + "\n");
	ent setModel("xmodel/crate_misc2a");
	ent attach("XMODEL/CRATE_MISC2A");
	logPrint("PROBE attach_case " + ent getAttachModelName(0) + "\n");
	hud = newHudElem();
	hud setShader("WHITE", 8, 8);
	logPrint("PROBE setshader_case ok\n");
	hud setShader("white");
	logPrint("PROBE setshader_one_arg ok\n");
	hud setText(&"MPSCRIPT_KILLCAM");
	logPrint("PROBE settext ok\n");
	hud.label = &"MPSCRIPT_KILLCAM";
	logPrint("PROBE label ok\n");
	objective_add(0, "current", (0, 0, 0), "WHITE");
	logPrint("PROBE objective_add ok\n");
	objective_icon(0, "white");
	logPrint("PROBE objective_icon ok\n");
	ent playSound("probe_never_indexed_alias");
	logPrint("PROBE playsound_new ok\n");
}
