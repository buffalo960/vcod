//	United Offensive's Base Assault bomb plant, defuse and round and map end,
//	for the A/B in crates/server/tests/gametypes_ab.rs. Runs stock UO bas.gsc
//	on mp_foy with two --save-scripted probes, allies then axis, under a
//	20 s bomb and a round limit of 1. The first level ends in bas's own
//	match-start restart. On the second the script breaches the three axis
//	bases with doDamage (as probe_bas does one), stands the allied player at
//	the axis hq's bomb looking at its trigger_lookat and has him hold use
//	(probe_use) through the 15 s plant; the axis player then holds use there
//	through the 10 s defuse. The allied player plants it again and goes on
//	to plant the ammo and fuel bases' bombs; the three blasts kill every axis
//	base, which ends the round for allies, the round limit ends the map in
//	intermission and exitLevel(false), and the empty rotation restarts it.
//	Recipe and measurements: docs/research/coduo-diff.md 22.

main()
{
	setCvar("scr_bas_bombtime", "20");
	setCvar("scr_bas_roundlimit", "1");
	thread choreograph();
	level thread place_players();
	maps\mp\gametypes\bas::main();
}

choreograph()
{
	// The level exitLevel(false) restarts into.
	if (getcvar("probe_ended") == "1")
	{
		logPrint("PROBE done " + getTime() + "\n");
		return;
	}
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
	hq = axis_base("hq");
	ammo = axis_base("ammo");
	fuel = axis_base("fuel");
	level thread watch_state(hq, ammo, fuel);
	allied = wait_player("allies");
	axis = wait_player("axis");
	logPrint("PROBE players " + allied getEntityNumber() + " " + axis getEntityNumber() + "\n");
	while (!isdefined(level.roundstarted) || !level.roundstarted)
		wait 0.05;
	wait 2;

	bases[0] = hq;
	bases[1] = ammo;
	bases[2] = fuel;
	for (i = 0; i < bases.size; i++)
		bases[i] doDamage(30000, bases[i].origin + (0, 0, 64), allied, allied, "MOD_GRENADE_SPLASH");
	wait 1;
	logPrint("PROBE breached " + getTime() + " " + hq.status + " " + ammo.status + " " + fuel.status + "\n");

	hold(allied, hq, "bomb_planted", 20);
	hold(axis, hq, "destroyed", 15);
	hold(allied, hq, "bomb_planted", 20);
	hold(allied, ammo, "bomb_planted", 20);
	hold(allied, fuel, "bomb_planted", 20);

	for (t = 0; t < 20 && !level.roundended; t += 0.05)
		wait 0.05;
	logPrint("PROBE round_ended " + getTime() + "\n");
	while (game["state"] != "intermission")
		wait 0.05;
	logPrint("PROBE intermission " + getTime() + "\n");
	setCvar("probe_ended", "1");
}

//	Stands p by the base's bomb looking at it, holds use until the base's
//	status reads want, then lets go and sends him home, clear of the blast.
hold(p, base, want, secs)
{
	if (!place_at_bomb(p, base))
	{
		logPrint("PROBE no_spot " + base.script_noteworthy + "\n");
		return;
	}
	logPrint("PROBE at " + base.script_noteworthy + " " + getTime() + " " + p getEntityNumber() + " " + p.origin + "\n");
	p setClientCvar("probe_use", "hold");
	for (t = 0; t < secs && base.status != want; t += 0.05)
		wait 0.05;
	p setClientCvar("probe_use", "0");
	logPrint("PROBE " + base.script_noteworthy + " " + getTime() + " " + base.status + "\n");
	wait 0.5;
	p setOrigin(home(p.pers["team"]));
}

//	Tries spots on rings around the bomb until one leaves p standing on the
//	ground within reach with a clear line from his eye to the bomb, and
//	turns him to face it.
place_at_bomb(p, base)
{
	bomb = base.objectivemodel.origin + (0, 0, 14);
	for (r = 40; r <= 120; r += 40)
	{
		for (a = 0; a < 360; a += 30)
		{
			spot = base.objectivemodel.origin + (cos(a) * r, sin(a) * r, 0);
			down = bulletTrace(spot + (0, 0, 32), spot - (0, 0, 64), false, undefined);
			if (down["fraction"] == 1)
				continue;
			p setOrigin(down["position"] + (0, 0, 1));
			wait 0.2;
			eye = p.origin + (0, 0, 60);
			if (!(p isOnGround()) || distance(p.origin, base.objectivemodel.origin) > 140)
			{
				logPrint("PROBE off " + r + " " + a + " " + p.origin + " " + p isOnGround() + "\n");
				continue;
			}
			look = bulletTrace(eye, bomb, false, undefined);
			if (look["fraction"] < 1)
			{
				logPrint("PROBE blocked " + r + " " + a + " " + look["position"] + " " + look["surfacetype"] + "\n");
				continue;
			}
			p setPlayerAngles(vectortoangles(bomb - eye));
			wait 0.2;
			return true;
		}
	}
	return false;
}

axis_base(kind)
{
	bases = getentarray("gmi_base", "targetname");
	for (i = 0; i < bases.size; i++)
		if (bases[i].script_team == "axis" && bases[i].script_noteworthy == kind)
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

//	One line whenever a watched value moves: each axis base's status, the
//	players' state and score, the team scores and the game state.
watch_state(hq, ammo, fuel)
{
	last = "";
	for (;;)
	{
		s = "hq " + str(hq.status) + " ammo " + str(ammo.status) + " fuel " + str(fuel.status);
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
