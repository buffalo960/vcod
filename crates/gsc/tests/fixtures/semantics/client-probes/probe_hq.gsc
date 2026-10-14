//	United Offensive's Headquarters radio capture, for the A/B in
//	crates/server/tests/gametypes_ab.rs. Runs stock UO hq.gsc on mp_cassino
//	with two --save-scripted probes, allies then axis. hq has no rounds: once
//	both teams are in, hq_obj_think shows a radio (a random one of the
//	seven) after its 30 s delay, and the script stands the allied player
//	on it until the 10 s hold captures it, then the axis player until the
//	8 s neutralize takes it down. Recipe and measurements:
//	docs/research/coduo-diff.md 21.

main()
{
	thread choreograph();
	level thread place_players();
	maps\mp\gametypes\hq::main();
}

choreograph()
{
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
	wait 2;

	allied setOrigin(home("allies"));
	axis setOrigin(radio.origin + (0, 0, 8));
	logPrint("PROBE axis_on " + getTime() + "\n");
	for (t = 0; t < 15 && radio.team == "allies"; t += 0.05)
		wait 0.05;
	logPrint("PROBE axis_neutralized " + getTime() + " " + radio.team + "\n");
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
	spawns = getentarray("mp_teamdeathmatch_spawn", "classname");
	if (team == "allies")
		return spawns[0].origin;
	return spawns[spawns.size - 1].origin;
}
