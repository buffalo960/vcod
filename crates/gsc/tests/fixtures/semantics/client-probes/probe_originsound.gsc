//	Whether a script_origin that playSound touched rides snapshots
//	(sound doc 9: playSound clears SVF_NOCLIENT and sets SVF_BROADCAST,
//	game.mp 0x5d910). mp_carentan dm. Three script_origins: far, out of
//	every PVS, sounded at 8 s and 10 s; near the dm intermission spot,
//	sounded at 12 s; near, never sounded. Logs each one's number:
//
//		PROBE origin <far|near|quiet> <num>
//		PROBE sound <far|near> <time>
//
//	Run by tools/run_probe.sh with a plain --net-probe spectator; its
//	entity lines say which numbers rode, from when, and with what event.

main()
{
	thread run();
	maps\mp\gametypes\dm::main();
}

run()
{
	wait 1;
	far = spawn("script_origin", (6000, 6000, 3000));
	logPrint("PROBE origin far " + far getEntityNumber() + "\n");
	near = spawn("script_origin", (420, -624, 184));
	logPrint("PROBE origin near " + near getEntityNumber() + "\n");
	quiet = spawn("script_origin", (440, -624, 184));
	logPrint("PROBE origin quiet " + quiet getEntityNumber() + "\n");
	wait 7;
	logPrint("PROBE sound far " + getTime() + "\n");
	far playSound("MP_bomb_plant");
	wait 2;
	logPrint("PROBE sound far " + getTime() + "\n");
	far playSound("MP_bomb_plant");
	wait 2;
	logPrint("PROBE sound near " + getTime() + "\n");
	near playSound("MP_bomb_plant");
	wait 3;
	logPrint("PROBE move far " + getTime() + "\n");
	far.origin = (6100, 6000, 3000);
	wait 2;
	logPrint("PROBE done " + getTime() + "\n");
}
