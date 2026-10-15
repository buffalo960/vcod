//	A panzerfaust round on the wire (docs/research/cod11-combat.md 13.6),
//	for the retail 1.1d server. The first player to spawn gets a loaded
//	panzerfaust in the second primary slot and, a second later, a switch
//	to it, logged as `PROBE weapon <time>`, and a teleport onto the
//	spawn point with the longest clear run along +x, or the run nearest
//	`probe_run` units (`PROBE spot`).
//
//	The shooter is `vcod --net-probe --probe-team axis --probe-weapsel
//	--capture-tag <t>`, which takes the switch and pulls the trigger from
//	15 s; a second `--probe-team spectator --probe-follow --save-snapshots
//	--capture-tag <t>` records the round and its explode. The committed
//	pairs (2026-10-15) are
//
//	  PORT=31304 PROBE_SECS=85 tools/run_probe.sh client-probes/probe_panzer \
//	      mp_carentan +set probe_run 1000
//	  vcod --net-probe 127.0.0.1:31304 --probe-team spectator --probe-follow \
//	      --save-snapshots --capture-tag panzer-wall --probe-secs 55
//	  vcod --net-probe 127.0.0.1:31304 --probe-team axis --probe-weapsel \
//	      --capture-tag shooter --probe-secs 48   # 3 s later, fixture discarded
//
//	and the same without `probe_run`, tagged `panzer`.

main()
{
	// dm precaches only the team weapons; a handover unprecached aborts.
	precacheItem("panzerfaust_mp");
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
				players[i] run();
				return;
			}
		}
		wait 0.05;
	}
}

run()
{
	self setWeaponSlotWeapon("primaryb", "panzerfaust_mp");
	self giveMaxAmmo("panzerfaust_mp");
	// A select sent with the spawn reaches the client before its first
	// playing snapshot, and is dropped.
	wait 1;
	self switchToWeapon("panzerfaust_mp");
	logPrint("PROBE weapon " + getTime() + "\n");
	// The probe client keeps its own view, which faces +x: stand on the
	// spawn point with the longest clear run that way, so the round flies
	// a while before it strikes.
	// With `probe_run` set, the spot whose run is nearest that many units
	// instead, for a round that strikes a wall.
	want = getCvarInt("probe_run");
	spots = getentarray("mp_deathmatch_spawn", "classname");
	best = spots[0].origin;
	score = -100000;
	run = 0;
	for (i = 0; i < spots.size; i++)
	{
		eye = spots[i].origin + (0, 0, 60);
		tr = bulletTrace(eye, eye + (8000, 0, 0), false, undefined);
		len = tr["fraction"] * 8000;
		s = len;
		if (want > 0)
		{
			s = 0 - (len - want);
			if (len < want)
				s = len - want;
		}
		if (s > score)
		{
			score = s;
			run = len;
			best = spots[i].origin;
		}
	}
	logPrint("PROBE spot " + getTime() + " " + best + " " + run + "\n");
	self setOrigin(best);
}
