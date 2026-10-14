//	United Offensive's Domination capture, for the A/B in
//	crates/server/tests/gametypes_ab.rs. Runs stock UO dom.gsc on mp_cassino
//	with two --save-scripted probes, allies then axis. The first level ends
//	in dom's own match-start restart; on the second the script stands the
//	allied player on flag5 (its trigger's script_timer is 3 s) until he
//	takes it, then sends him back to his spawn and the axis player onto the
//	flag to take it back. Recipe and measurements:
//	docs/research/coduo-diff.md 21.

main()
{
	thread choreograph();
	level thread place_players();
	maps\mp\gametypes\dom::main();
}

choreograph()
{
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
	flag = getent("flag5", "targetname");
	spot = getent("flag5_neutral", "targetname").origin + (0, 0, 8);
	level thread watch_state(flag);
	allied = wait_player("allies");
	axis = wait_player("axis");
	logPrint("PROBE players " + allied getEntityNumber() + " " + axis getEntityNumber() + "\n");
	while (!isdefined(level.roundstarted) || !level.roundstarted)
		wait 0.05;
	wait 2;

	allied setOrigin(spot);
	logPrint("PROBE allied_on " + getTime() + " " + allied.origin + "\n");
	for (t = 0; t < 10 && flag.team != "allies"; t += 0.05)
		wait 0.05;
	logPrint("PROBE allied_capped " + getTime() + " " + flag.team + "\n");
	wait 2;

	allied setOrigin(home("allies"));
	axis setOrigin(spot);
	logPrint("PROBE axis_on " + getTime() + " " + axis.origin + "\n");
	for (t = 0; t < 10 && flag.team != "axis"; t += 0.05)
		wait 0.05;
	logPrint("PROBE axis_capped " + getTime() + " " + flag.team + "\n");
	wait 3;
	logPrint("PROBE done " + getTime() + "\n");
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

//	One line whenever a watched value moves: the flag's owner and cappers,
//	the players' scores, the team scores.
watch_state(flag)
{
	last = "";
	for (;;)
	{
		s = "flag5 " + flag.team + " capping=" + flag.capping + " allies=" + flag.allies + " axis=" + flag.axis;
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
		{
			p = players[i];
			s = s + " | " + p getEntityNumber() + " " + p.sessionstate + " sc=" + p.score;
		}
		s = s + " | allies " + getTeamScore("allies") + " axis " + getTeamScore("axis");
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
