//	United Offensive's Capture the Flag pickup and capture, for the A/B in
//	crates/server/tests/gametypes_ab.rs. Runs stock UO ctf.gsc on mp_cassino
//	with two --save-scripted probes, allies then axis. The first level ends
//	in ctf's own match-start restart; on the second the script puts the
//	allied player on the axis flag (the trigger_multiple pickup) and then
//	into the axis flag's goal (the capture). Recipe and measurements:
//	docs/research/coduo-diff.md 22.

main()
{
	thread choreograph();
	level thread place_players();
	maps\mp\gametypes\ctf::main();
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
	level thread watch_state();
	attacker = wait_player("allies");
	defender = wait_player("axis");
	logPrint("PROBE players " + attacker getEntityNumber() + " " + defender getEntityNumber() + "\n");
	wait 2;

	flag = level.axis_flag;
	attacker setOrigin(flag.trigger.origin + (0, 0, 8));
	logPrint("PROBE to_flag " + getTime() + " " + attacker.origin + "\n");
	for (t = 0; t < 10 && !isdefined(attacker.hasflag); t += 0.05)
		wait 0.05;
	logPrint("PROBE picked_up " + getTime() + " " + isdefined(attacker.hasflag) + "\n");
	wait 2;

	attacker setOrigin(flag.goal.origin + (0, 0, 8));
	logPrint("PROBE to_goal " + getTime() + " " + attacker.origin + "\n");
	for (t = 0; t < 10 && getTeamScore("allies") == 0; t += 0.05)
		wait 0.05;
	logPrint("PROBE captured " + getTime() + " " + getTeamScore("allies") + "\n");
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

//	One line whenever a watched value moves: both flags and their carrier,
//	the players' carrier fields and score, the team scores.
watch_state()
{
	last = "";
	for (;;)
	{
		s = "allies_flag " + flag_str(level.allies_flag) + " axis_flag " + flag_str(level.axis_flag);
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
		{
			p = players[i];
			s = s + " | " + p getEntityNumber() + " " + p.sessionstate + " st=" + str(p.statusicon) + " sc=" + p.score + " flag=" + isdefined(p.hasflag);
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

flag_str(f)
{
	if (!isdefined(f))
		return "undef";
	s = "moved=" + f.moved;
	if (isdefined(f.carried_by))
		s = s + " by=" + f.carried_by getEntityNumber();
	return s;
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
	return getentarray("mp_uo_spawn_" + team, "classname")[0].origin;
}
