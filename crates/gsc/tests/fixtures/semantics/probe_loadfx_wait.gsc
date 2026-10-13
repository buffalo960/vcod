// loadFx after the script's first wait (docs/research/cod11-events-and-fx.md
// 2, "loadFx"). The gametype's main loads one effect while the level
// initializes; a thread waits a frame and loads it again, in another case,
// then loads a new one, which retail refuses with a script error.

main()
{
	fx = loadfx("fx/explosions/mp_bomb.efx");
	logPrint("PROBE loadfx_init " + fx + "\n");
	thread late();
	maps\mp\gametypes\dm::main();
}

late()
{
	wait 0.05;
	again = loadfx("fx/explosions/mp_bomb.efx");
	logPrint("PROBE loadfx_again " + again + "\n");
	upper = loadfx("FX/Explosions/MP_BOMB.efx");
	logPrint("PROBE loadfx_case " + upper + "\n");
	logPrint("PROBE at loadfx_new\n");
	fresh = loadfx("fx/smoke/chimneysmoke.efx");
	logPrint("PROBE loadfx_new " + fresh + "\n");
}
