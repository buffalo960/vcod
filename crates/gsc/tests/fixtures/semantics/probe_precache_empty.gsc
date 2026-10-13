// Empty names (docs/research/cod11-gsc-language.md, "Precache builtins"):
// precacheString skips one, precacheShader refuses one.

main()
{
	maps\mp\gametypes\dm::main();
	logPrint("PROBE at empty_model\n");
	precacheModel("");
	logPrint("PROBE empty_model ok\n");
	logPrint("PROBE at empty_string\n");
	precacheString(&"");
	logPrint("PROBE empty_string ok\n");
	logPrint("PROBE at empty_shader\n");
	precacheShader("");
	logPrint("PROBE empty_shader ok\n");
}
