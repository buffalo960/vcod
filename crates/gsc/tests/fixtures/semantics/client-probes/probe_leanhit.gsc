//	A leaning target's hit pose and a leaning shooter's rounds
//	(docs/research/cod11-combat.md 2.1, 3, 16.4). Stock tdm, any patch.
//
//	Two `--probe-uo-weapon direct` clients: an allies shooter
//	(`--probe-weapon springfield_mp`) and an axis target. The site is the
//	first tdm spawn point past the first with 256 units of level ground
//	clear on its facing (probe_scriptmodel's pair site): the shooter
//	stands on it, the target 256 ahead facing back, both held there and
//	their views set every frame. The script drives both clients' inputs
//	through `iPrintLn("PROBE_IN <buttons> <wbuttons> <up>")`, so every
//	phase starts on the server's clock. Once both play, the target lies
//	prone and stands again, which sets his legs and swings his torso onto
//	his view whichever spawn point tdm gave him; the site has 80 units of
//	level ground clear behind him for the prone body:
//
//	PROBE site <time> <shooter spot> <target spot> <yaw>
//	PROBE phase <time> <name>
//
//	Each phase holds its stance and lean, then a grid of level
//	`bulletTrace`s (hitCharacters on) crosses the target: `f` rows run
//	along the shooter's facing from 64 in front of the target to 80
//	behind, 11 across from 30 left to 30 right; `s` rows run across from
//	48 right of him to 48 left, 15 along from 30 in front to 54 behind.
//	A row is a height off the target's feet, 4 to 76 every 6. Each cell
//	is the distance from the ray's start to where it stopped on the
//	target, or `-` when it stopped elsewhere or ran on:
//
//	PROBE grid <time> <phase> <f|s> <height> <cell>...
//
//	Then the shooter, sighted all along, taps the trigger once, aimed
//	from his own leaned eye (the 19.2 across and 5.5 down of a full lean)
//	at the target's chest moved half his lean. The damage callback logs
//	and takes no health:
//
//	PROBE hit <time> <phase> <team> dmg <amount> loc <location> point <point>

main()
{
	thread run();
	maps\mp\gametypes\tdm::main();
	level.callbackPlayerDamage = ::probe_damage;
}

probe_damage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc)
{
	who = self getEntityNumber();
	if (isdefined(self.pers["team"]))
		who = self.pers["team"];
	logPrint("PROBE hit " + gettime() + " " + level.probe_phase + " " + who + " dmg " + iDamage + " loc " + sHitLoc + " point " + vPoint + "\n");
}

ground_at(p)
{
	tr = bullettrace(p + (0, 0, 64), p + (0, 0, -128), false, undefined);
	return tr["position"];
}

//	probe_scriptmodel's pick_site, with 80 units of level ground clear at
//	8 and 20 up past the target's spot.
pick_site()
{
	spawns = getentarray("mp_teamdeathmatch_spawn", "classname");
	if (!spawns.size)
		spawns = getentarray("mp_tdm_spawn", "classname");
	for (i = 1; i < spawns.size; i++)
	{
		yaw = spawns[i].angles[1];
		fwd = anglestoforward((0, yaw, 0));
		a = ground_at(spawns[i].origin + (0, 0, 8));
		b = ground_at(a + (fwd[0] * 256, fwd[1] * 256, 0));
		dz = b[2] - a[2];
		if (dz > 2 || dz < -2)
			continue;
		if (bullettrace(a + (0, 0, 60), b + (0, 0, 60), false, undefined)["fraction"] < 1)
			continue;
		if (bullettrace(a + (0, 0, 20), b + (0, 0, 20), false, undefined)["fraction"] < 1)
			continue;
		mid = ground_at(a + (fwd[0] * 128, fwd[1] * 128, 0));
		dz = mid[2] - a[2];
		if (dz > 2 || dz < -2)
			continue;
		c = ground_at(b + (fwd[0] * 80, fwd[1] * 80, 0));
		dz = c[2] - a[2];
		if (dz > 2 || dz < -2)
			continue;
		if (bullettrace(b + (0, 0, 8), c + (0, 0, 8), false, undefined)["fraction"] < 1)
			continue;
		if (bullettrace(b + (0, 0, 20), c + (0, 0, 20), false, undefined)["fraction"] < 1)
			continue;
		site = spawnstruct();
		site.a = a + (0, 0, 0.125);
		site.b = b + (0, 0, 0.125);
		site.yaw = yaw;
		site.fwd = fwd;
		return site;
	}
	return undefined;
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

//	One phase: target and shooter `wbuttons` (0x10 lean left, 0x20 lean
//	right, 0x40 prone, 0x80 crouch) and how long it settles.
phase(name, twb, swb, settle)
{
	p = spawnstruct();
	p.name = name;
	p.twb = twb;
	p.swb = swb;
	p.settle = settle;
	return p;
}

phases()
{
	ps = [];
	ps[ps.size] = phase("level", 0, 0, 1.5);
	ps[ps.size] = phase("t_left", 16, 0, 1.5);
	ps[ps.size] = phase("t_right", 32, 0, 1.5);
	ps[ps.size] = phase("t_crouch", 128, 0, 1.5);
	ps[ps.size] = phase("t_crouch_left", 144, 0, 1.5);
	ps[ps.size] = phase("t_crouch_right", 160, 0, 1.5);
	ps[ps.size] = phase("t_prone", 64, 0, 2.5);
	ps[ps.size] = phase("t_prone_left", 80, 0, 1.5);
	ps[ps.size] = phase("t_prone_right", 96, 0, 1.5);
	ps[ps.size] = phase("t_stand", 0, 0, 2.5);
	ps[ps.size] = phase("s_left", 0, 16, 1.5);
	ps[ps.size] = phase("s_right", 0, 32, 1.5);
	ps[ps.size] = phase("s_left_t_right", 32, 16, 1.5);
	return ps;
}

//	`up` -127 under crouch or prone, as a client holds it.
send_in(player, buttons, wb)
{
	up = 0;
	if (wb & 192)
		up = -127;
	player iprintln("PROBE_IN " + buttons + " " + wb + " " + up);
}

//	The eye offset of a full lean `wb` along `yaw` (AddLeanToPosition).
lean_off(wb, yaw)
{
	s = 0;
	if (wb & 16)
		s = -1;
	if (wb & 32)
		s = 1;
	if (s == 0)
		return (0, 0, 0);
	r = anglestoright((0, yaw, 0));
	return (r[0] * 19.225 * s, r[1] * 19.225 * s, -5.513);
}

cell(start, end, target)
{
	tr = bullettrace(start, end, true, undefined);
	if (!isdefined(tr["entity"]) || tr["entity"] != target || tr["fraction"] >= 1)
		return "-";
	return "" + tr["fraction"] * distance(start, end);
}

grid(name, site, target)
{
	fwd = site.fwd;
	r = anglestoright((0, site.yaw, 0));
	o = site.b;
	for (h = 4; h <= 76; h += 6)
	{
		line = "PROBE grid " + gettime() + " " + name + " f " + h;
		for (x = -30; x <= 30; x += 6)
		{
			c = o + (r[0] * x, r[1] * x, h);
			line = line + " " + cell(c - (fwd[0] * 64, fwd[1] * 64, 0), c + (fwd[0] * 80, fwd[1] * 80, 0), target);
		}
		logPrint(line + "\n");
	}
	for (h = 4; h <= 76; h += 6)
	{
		line = "PROBE grid " + gettime() + " " + name + " s " + h;
		for (y = -30; y <= 54; y += 6)
		{
			c = o + (fwd[0] * y, fwd[1] * y, h);
			line = line + " " + cell(c + (r[0] * 48, r[1] * 48, 0), c - (r[0] * 48, r[1] * 48, 0), target);
		}
		logPrint(line + "\n");
	}
}

//	Holds both players on their spots and the shooter's aim, every frame.
hold(site)
{
	for (;;)
	{
		shooter = team_player("allies");
		target = team_player("axis");
		if (isdefined(shooter) && isdefined(target))
		{
			if (distance(shooter.origin, site.a) > 0.5)
				shooter setorigin(site.a);
			off = target.origin - site.b;
			if (level.probe_twb & 64)
				off = (off[0], off[1], 0);
			if (length(off) > 0.5)
				target setorigin(site.b);
			shooter setWeaponSlotClipAmmo("primary", 5);
			shooter giveMaxAmmo(shooter getcurrentweapon());
			eye = site.a + (0, 0, 60) + lean_off(level.probe_swb, site.yaw);
			fwd = site.fwd;
			if (level.probe_twb & 64)
				at = site.b + (fwd[0] * 20, fwd[1] * 20, 8);
			else if (level.probe_twb & 128)
				at = site.b + (0, 0, 30);
			else
				at = site.b + (0, 0, 44);
			t = lean_off(level.probe_twb, site.yaw + 180);
			at = at + (t[0] * 0.5, t[1] * 0.5, t[2] * 0.5);
			shooter setplayerangles(vectortoangles(at - eye));
			target setplayerangles((0, site.yaw + 180, 0));
		}
		wait 0.05;
	}
}

run()
{
	level.probe_phase = "none";
	level.probe_twb = 0;
	level.probe_swb = 0;
	wait 1;
	site = pick_site();
	if (!isdefined(site))
	{
		logPrint("PROBE site none\n");
		return;
	}
	logPrint("PROBE site " + gettime() + " " + site.a + " " + site.b + " " + site.yaw + "\n");
	thread hold(site);
	for (;;)
	{
		if (isdefined(team_player("allies")) && isdefined(team_player("axis")))
			break;
		wait 0.05;
	}
	//	The target lies prone and stands, so his legs and torso face his
	//	view whatever yaw his spawn point gave them (cod11-combat.md 16.4);
	//	the shooter's rifle is raised and sighted.
	level.probe_twb = 64;
	send_in(team_player("axis"), 0, 64);
	send_in(team_player("allies"), 16, 0);
	wait 2.5;
	level.probe_twb = 0;
	send_in(team_player("axis"), 0, 0);
	wait 2.5;
	//	The phases start on a fixed level time, so the scoped sway and the
	//	idle clip read the same clock on any server however late the
	//	clients joined.
	while (gettime() < 20000)
		wait 0.05;
	ps = phases();
	for (i = 0; i < ps.size; i++)
	{
		p = ps[i];
		shooter = team_player("allies");
		target = team_player("axis");
		if (!isdefined(shooter) || !isdefined(target))
			return;
		level.probe_phase = p.name;
		level.probe_twb = p.twb;
		level.probe_swb = p.swb;
		logPrint("PROBE phase " + gettime() + " " + p.name + "\n");
		send_in(target, 0, p.twb);
		send_in(shooter, 16, p.swb);
		wait p.settle;
		grid(p.name, site, target);
		send_in(shooter, 17, p.swb);
		wait 0.05;
		send_in(shooter, 16, p.swb);
		wait 1.5;
	}
	logPrint("PROBE done " + gettime() + "\n");
}
