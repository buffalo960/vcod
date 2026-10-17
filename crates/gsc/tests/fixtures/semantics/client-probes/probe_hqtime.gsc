//	United Offensive's Headquarters time limit, for the A/B in
//	crates/server/tests/gametypes_ab.rs. Runs stock UO hq.gsc on mp_cassino
//	with two --save-scripted probes, allies then axis, under a half-minute
//	time limit. Nobody takes the radio: the time limit ends the map in
//	intermission and exitLevel(false), and the empty rotation restarts it.
//	Recipe and measurements: docs/research/coduo-diff.md 22.

main()
{
	setCvar("scr_hq_timelimit", "0.5");
	thread choreograph();
	level thread place_players();
	maps\mp\gametypes\hq::main();
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
	level thread watch_state();
	attacker = wait_player("allies");
	defender = wait_player("axis");
	logPrint("PROBE players " + attacker getEntityNumber() + " " + defender getEntityNumber() + "\n");
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

//	One line whenever a watched value moves: the players' state, score and
//	deaths, the team scores and the game state.
watch_state()
{
	last = "";
	for (;;)
	{
		s = "";
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
		{
			p = players[i];
			s = s + " | " + p getEntityNumber() + " " + p.sessionstate + " st=" + str(p.statusicon) + " sc=" + p.score + " d=" + p.deaths;
		}
		s = s + " | allies " + getTeamScore("allies") + " axis " + getTeamScore("axis") + " " + str(game["state"]) + " ended=" + str(level.roundended) + " map=" + str(level.mapended);
		if (s != last)
		{
			logPrint("PROBE state " + s + "\n");
			last = s;
		}
		wait 0.05;
	}
}

str(v)
{
	if (!isdefined(v))
		return "undef";
	return v;
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
		logPrint("PROBE placed " + self getEntityNumber() + " " + self.pers["team"] + " " + self.origin + " " + self.angles + "\n");
		wait 0.5;
		logPrint("PROBE settled " + self getEntityNumber() + " " + self.angles + "\n");
	}
}

home(team)
{
	return getentarray("mp_uo_spawn_" + team, "classname")[0].origin;
}
