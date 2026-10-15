//	United Offensive's Headquarters wave scoring and map end, for the A/B in
//	crates/server/tests/gametypes_ab.rs. Runs stock UO hq.gsc on mp_cassino
//	with two --save-scripted probes, allies then axis, under a score limit
//	of 90. Once a radio shows (a random one of the seven, after its 30 s
//	delay) the script stands the allied player on it until the 10 s hold
//	captures it and sends him home; the radio then scores 45 for allies at
//	the end of each 45 s reinforcement wave, the second wave reaches the
//	score limit and the map ends in intermission and exitLevel(false), and
//	the empty rotation restarts it. Recipe and measurements:
//	docs/research/coduo-diff.md 22.

main()
{
	setCvar("scr_hq_scorelimit", "90");
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
	allied = wait_player("allies");
	axis = wait_player("axis");
	logPrint("PROBE players " + allied getEntityNumber() + " " + axis getEntityNumber() + "\n");
	radio = wait_radio();
	logPrint("PROBE radio " + getTime() + "\n");
	wait 1;

	allied setOrigin(radio.origin + (0, 0, 8));
	logPrint("PROBE allied_on " + getTime() + "\n");
	for (t = 0; t < 15 && radio.team != "allies"; t += 0.05)
		wait 0.05;
	logPrint("PROBE allied_capped " + getTime() + " " + radio.team + "\n");
	wait 1;
	allied setOrigin(home("allies"));

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

wait_radio()
{
	for (;;)
	{
		if (isdefined(level.radio))
			for (i = 0; i < level.radio.size; i++)
				if (!level.radio[i].hidden)
					return level.radio[i];
		wait 0.05;
	}
}

//	One line whenever a watched value moves: each shown radio's team, the
//	players' scores, the team scores. No entity numbers: vcod spawns none of
//	the map's vehicles, which sit ahead of the radios.
watch_state()
{
	last = "";
	for (;;)
	{
		s = "radios";
		if (isdefined(level.radio))
			for (i = 0; i < level.radio.size; i++)
			{
				r = level.radio[i];
				if (!r.hidden)
					s = s + " " + r.team;
			}
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
		{
			p = players[i];
			s = s + " | " + p getEntityNumber() + " " + p.sessionstate + " sc=" + p.score;
		}
		s = s + " | allies " + getTeamScore("allies") + " axis " + getTeamScore("axis") + " " + str(game["state"]) + " map=" + str(level.mapended);
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
		logPrint("PROBE placed " + self getEntityNumber() + " " + self.pers["team"] + " " + self.origin + "\n");
	}
}

home(team)
{
	spawns = getentarray("mp_teamdeathmatch_spawn", "classname");
	if (team == "allies")
		return spawns[0].origin;
	return spawns[spawns.size - 1].origin;
}
