//	United Offensive's Base Assault breach, for the A/B in
//	crates/server/tests/gametypes_ab.rs. Runs stock UO bas.gsc on mp_foy with
//	two --save-scripted probes, allies then axis. The first level ends in
//	bas's own match-start restart; on the second the script deals an axis
//	base one 30000-point MOD_GRENADE_SPLASH hit from the allied player
//	through doDamage, which the base's own waittill("damage") takes as a
//	breach (its health is 24500). No tank is needed. Recipe and
//	measurements: docs/research/coduo-diff.md 21.

main()
{
	thread choreograph();
	level thread place_players();
	maps\mp\gametypes\bas::main();
}

choreograph()
{
	if (getcvar("mapname") != "mp_foy")
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
	base = axis_base();
	level thread watch_state(base);
	allied = wait_player("allies");
	axis = wait_player("axis");
	logPrint("PROBE players " + allied getEntityNumber() + " " + axis getEntityNumber() + "\n");
	while (!isdefined(level.roundstarted) || !level.roundstarted)
		wait 0.05;
	wait 2;

	logPrint("PROBE hit " + getTime() + " " + base.script_noteworthy + " " + base.health + "\n");
	base doDamage(30000, base.origin + (0, 0, 64), allied, allied, "MOD_GRENADE_SPLASH");
	for (t = 0; t < 5 && base.status != "destroyed"; t += 0.05)
		wait 0.05;
	logPrint("PROBE breached " + getTime() + " " + base.status + " " + base.health + "\n");
	wait 5;
	logPrint("PROBE done " + getTime() + "\n");
}

axis_base()
{
	bases = getentarray("gmi_base", "targetname");
	for (i = 0; i < bases.size; i++)
		if (bases[i].script_team == "axis")
			return bases[i];
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

//	One line whenever a watched value moves: the base's status and health,
//	the players' scores, the team scores.
watch_state(base)
{
	last = "";
	for (;;)
	{
		s = "base " + str(base.status) + " " + str(base.health);
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
	return getentarray("mp_gmi_bas_" + team + "_spawn", "classname")[0].origin;
}
