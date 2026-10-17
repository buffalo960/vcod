//	Whether a lean moves the eye a snapshot is culled from
//	(docs/research/cod11-combat.md 3.6, `SV_BuildClientSnapshot`'s
//	`AddLeanToPosition`). Stock tdm, mp_carentan, any patch.
//
//	Two `--probe-uo-weapon direct` clients: an allies watcher held at
//	(1368, 2608, -23.875) facing 90, and an axis target held at (1080,
//	3488, -23.875). A cluster boundary runs by the watcher's eye: level,
//	and leaned right into the wall beside him, it sits in a cluster that
//	sees the target; leaned left it crosses into one that does not. The
//	script drives the watcher's lean through `iPrintLn("PROBE_IN <buttons>
//	<wbuttons> <up>")` in phases of two seconds from level time 15000; the
//	watcher's snapshots carry the target's entity or not:
//
//	PROBE phase <time> <name> <wbuttons>
//	PROBE done <time>

main()
{
	thread run();
	maps\mp\gametypes\tdm::main();
}

team_player(team)
{
	players = getentarray("player", "classname");
	for (i = 0; i < players.size; i++)
	{
		p = players[i];
		if (isdefined(p.sessionstate) && p.sessionstate == "playing" && isdefined(p.pers["team"]) && p.pers["team"] == team)
			return p;
	}
	return undefined;
}

hold()
{
	at = (1368, 2608, -23.875);
	there = (1080, 3488, -23.875);
	for (;;)
	{
		watcher = team_player("allies");
		target = team_player("axis");
		if (isdefined(watcher))
		{
			if (distance(watcher.origin, at) > 0.5)
				watcher setorigin(at);
			watcher setplayerangles((0, 90, 0));
		}
		if (isdefined(target))
		{
			if (distance(target.origin, there) > 0.5)
				target setorigin(there);
			target setplayerangles((0, 270, 0));
		}
		wait 0.05;
	}
}

phase(name, wb)
{
	logPrint("PROBE phase " + gettime() + " " + name + " " + wb + "\n");
	team_player("allies") iprintln("PROBE_IN 0 " + wb + " 0");
	wait 2;
}

run()
{
	thread hold();
	for (;;)
	{
		if (isdefined(team_player("allies")) && isdefined(team_player("axis")))
			break;
		wait 0.05;
	}
	while (gettime() < 15000)
		wait 0.05;
	phase("level", 0);
	phase("right", 32);
	phase("level2", 0);
	phase("left", 16);
	phase("right2", 32);
	phase("level3", 0);
	logPrint("PROBE done " + gettime() + "\n");
}
