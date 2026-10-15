//	United Offensive's Domination bonus and round and map end, for the A/B
//	in crates/server/tests/gametypes_ab.rs. Runs stock UO dom.gsc on
//	mp_cassino with two --save-scripted probes, allies then axis, under a
//	round limit of 1. The first level ends in dom's own match-start restart.
//	On the second the script stands the allied player on each of the five
//	flags in turn until he holds it; the fifth makes Flag_AllCapturedThink
//	end the round for allies with scr_dom_domination_points on top, the
//	round limit ends the map in intermission and exitLevel(false), and the
//	empty rotation restarts it. Recipe and measurements:
//	docs/research/coduo-diff.md 22.

main()
{
	setCvar("scr_dom_roundlimit", "1");
	thread choreograph();
	level thread place_players();
	maps\mp\gametypes\dom::main();
}

choreograph()
{
	// The level exitLevel(false) restarts into.
	if (getcvar("probe_ended") == "1")
	{
		logPrint("PROBE done " + getTime() + "\n");
		return;
	}
	if (getcvar("mapname") != "mp_cassino")
	{
		logPrint("PROBE unsupported " + getcvar("mapname") + "\n");
		return;
	}
	wait 1;
	if (!game["matchstarted"])
	{
		logPrint("PROBE prematch\n");
		return;
	}
	level thread watch_state();
	allied = wait_player("allies");
	axis = wait_player("axis");
	logPrint("PROBE players " + allied getEntityNumber() + " " + axis getEntityNumber() + "\n");
	while (!isdefined(level.roundstarted) || !level.roundstarted)
		wait 0.05;
	wait 2;

	for (q = 1; q <= 5; q++)
	{
		flag = getent("flag" + q, "targetname");
		allied setOrigin(getent("flag" + q + "_neutral", "targetname").origin + (0, 0, 8));
		logPrint("PROBE allied_on " + q + " " + getTime() + " " + allied.origin + "\n");
		for (t = 0; t < 15 && flag.team != "allies"; t += 0.05)
			wait 0.05;
		logPrint("PROBE allied_capped " + q + " " + getTime() + " " + flag.team + "\n");
		wait 1;
	}
	allied setOrigin(home("allies"));
	for (t = 0; t < 5 && !level.roundended; t += 0.05)
		wait 0.05;
	logPrint("PROBE round_ended " + getTime() + " " + level.roundended + "\n");
	while (game["state"] != "intermission")
		wait 0.05;
	logPrint("PROBE intermission " + getTime() + "\n");
	setCvar("probe_ended", "1");
}

wait_player(team)
{
	for (;;)
	{
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
		{
			p = players[i];
			if (isdefined(p.pers["team"]) && p.pers["team"] == team && p.sessionstate == "playing")
				return p;
		}
		wait 0.05;
	}
}

//	One line whenever a watched value moves: every flag's owner, the
//	players' state and score, the team scores and the game state.
watch_state()
{
	last = "";
	for (;;)
	{
		s = "flags";
		for (q = 1; q <= 5; q++)
			s = s + " " + getent("flag" + q, "targetname").team;
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
		{
			p = players[i];
			s = s + " | " + p getEntityNumber() + " " + p.sessionstate + " sc=" + p.score;
		}
		s = s + " | allies " + getTeamScore("allies") + " axis " + getTeamScore("axis") + " " + game["state"] + " ended=" + level.roundended + " map=" + level.mapended;
		if (s != last)
		{
			logPrint("PROBE state " + s + "\n");
			last = s;
		}
		wait 0.05;
	}
}

//	Every spawn is moved onto a fixed spot per team, so what each probe sees
//	of the other is not the spawn script's random pick.
place_players()
{
	for (;;)
	{
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
		{
			if (!isdefined(players[i].probe_watch))
			{
				players[i].probe_watch = true;
				players[i] thread place_on_spawn();
			}
		}
		wait 0.05;
	}
}

place_on_spawn()
{
	for (;;)
	{
		self waittill("spawned");
		wait 0.05;
		if (self.sessionstate != "playing")
			continue;
		self setOrigin(home(self.pers["team"]));
		self setPlayerAngles((0, 0, 0));
		logPrint("PROBE placed " + self getEntityNumber() + " " + self.pers["team"] + " " + self.origin + "\n");
	}
}

home(team)
{
	return getentarray("mp_uo_spawn_" + team, "classname")[0].origin;
}
