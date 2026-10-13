//	musicPlay, musicStop and soundFade on the wire and by ear
//	(docs/research/cod11-sound-system.md 10). Once the first player is
//	playing, the steps run on this clock, each logged as `PROBE <step>`
//	and printed on screen:
//
//	 0 s  music       musicPlay("codtheme")
//	 5 s  fade_low    soundFade(0.2, 3): every sound down to a fifth over 3 s
//	11 s  fade_up     soundFade(1, 2): back up over 2 s
//	16 s  music_stop  musicStop(10): the track ramps out over 10 s
//	28 s  fade_out    soundFade(0, 2): everything fades out, then stops
//	33 s  fade_back   soundFade(1): new sounds play again, the ambient stays off
//
//	On the wire: `o codtheme`, `q 0.200000 3000`, `q 1.000000 2000`,
//	`p 10000`, `q 0.000000 2000`, `q 1.000000 0`, each `q` with retail's
//	trailing newline.

main()
{
	thread watch();
	maps\mp\gametypes\dm::main();
}

watch()
{
	for (;;)
	{
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
		{
			if (isdefined(players[i].sessionstate) && players[i].sessionstate == "playing")
			{
				steps();
				return;
			}
		}
		wait 0.05;
	}
}

steps()
{
	step("music");
	musicplay("codtheme");
	wait 5;
	step("fade_low");
	soundfade(0.2, 3);
	wait 6;
	step("fade_up");
	soundfade(1, 2);
	wait 5;
	step("music_stop");
	musicstop(10);
	wait 12;
	step("fade_out");
	soundfade(0, 2);
	wait 5;
	step("fade_back");
	soundfade(1);
}

step(name)
{
	logprint("PROBE " + name + " " + gettime() + "\n");
	iprintln("soundfade probe: " + name);
}
