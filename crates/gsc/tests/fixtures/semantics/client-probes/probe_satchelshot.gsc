//	United Offensive: a satchel charge shot off, and the satchel its blast
//	sets off (docs/research/coduo-diff.md 21, "Satchel"). Stock tdm, UO
//	only. Client 0 (`--probe-uo-weapon direct`) is held on a spot and its
//	view set every frame: the first tdm spawn point past the first with
//	`probe_clear` (default 384) units of level ground clear on its facing,
//	or (`probe_x`, `probe_y`, `probe_z`) facing `probe_yaw` when `probe_x`
//	is set. A second in it gets one satchel, switched to; it throws one
//	looking 10 degrees up along the facing (the trigger held 0.7 s through
//	`iPrintLn("PROBE_IN <buttons> <wbuttons> <up>")`), is handed a second
//	once the spent one is gone, and throws it `probe_turn` (default 30)
//	degrees to the left. Then it gets a kar98k, sights and fires once at
//	the second satchel's box, aimed from its eye at the satchel's origin 3
//	up; with `probe_shoot 0` it holds its fire and the first satchel's own
//	fuse goes first. Every satchel alive is logged every frame:
//
//	PROBE spot <time> <spot> <yaw>
//	PROBE throw <time> <entity number> <origin>
//	PROBE miss <time> <entity number> <origin>
//	PROBE fire <time> <entity number> <origin> <angles>
//	PROBE hit <time> <entity number> dmg <amount> mod <mod>
//	PROBE done <time>
//
//	A player's damage callback only logs. The fixture pairs the log with
//	the shooter's `VEH` missile lines.

main()
{
	precacheItem("satchelcharge_mp");
	precacheItem("kar98k_mp");
	thread run();
	maps\mp\gametypes\tdm::main();
	level.callbackPlayerDamage = ::probe_damage;
}

probe_damage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc)
{
	logPrint("PROBE hit " + gettime() + " " + self getEntityNumber() + " dmg " + iDamage + " mod " + sMeansOfDeath + "\n");
}

ground_at(p)
{
	tr = bullettrace(p + (0, 0, 64), p + (0, 0, -128), false, undefined);
	return tr["position"];
}

first_player()
{
	players = getentarray("player", "classname");
	for (i = 0; i < players.size; i++)
	{
		p = players[i];
		if (p getEntityNumber() == 0 && isdefined(p.sessionstate) && p.sessionstate == "playing")
			return p;
	}
	return undefined;
}

//	The spot and its yaw, as an array.
find_spot()
{
	if (getcvar("probe_x") != "")
	{
		s[0] = (getcvarfloat("probe_x"), getcvarfloat("probe_y"), getcvarfloat("probe_z") + 0.125);
		s[1] = getcvarfloat("probe_yaw");
		return s;
	}
	clear = 384;
	if (getcvar("probe_clear") != "")
		clear = getcvarint("probe_clear");
	spawns = getentarray("mp_teamdeathmatch_spawn", "classname");
	for (i = 1; i < spawns.size; i++)
	{
		yaw = spawns[i].angles[1];
		fwd = anglestoforward((0, yaw, 0));
		a = ground_at(spawns[i].origin + (0, 0, 8));
		level_ok = true;
		for (d = 64; d <= clear && level_ok; d += 64)
		{
			b = ground_at(a + (fwd[0] * d, fwd[1] * d, 0));
			dz = b[2] - a[2];
			if (dz > 2 || dz < -2)
				level_ok = false;
		}
		if (!level_ok)
			continue;
		far = a + (fwd[0] * clear, fwd[1] * clear, 0);
		if (bullettrace(a + (0, 0, 60), far + (0, 0, 60), false, undefined)["fraction"] < 1)
			continue;
		s[0] = a + (0, 0, 0.125);
		s[1] = yaw;
		return s;
	}
	return undefined;
}

press(p, buttons)
{
	p iprintln("PROBE_IN " + buttons + " 0 0");
}

run()
{
	wait 1;
	s = find_spot();
	if (!isdefined(s))
	{
		logPrint("PROBE spot none " + getcvar("mapname") + "\n");
		return;
	}
	spot = s[0];
	yaw = s[1];
	logPrint("PROBE spot " + gettime() + " " + spot + " " + yaw + "\n");
	shoot = getcvar("probe_shoot") != "0";
	turn = 30;
	if (getcvar("probe_turn") != "")
		turn = getcvarint("probe_turn");
	placed = undefined;
	thrown = 0;
	known = [];
	target = undefined;
	view = (-10, yaw, 0);
	for (;;)
	{
		p = first_player();
		if (!isdefined(p))
		{
			wait 0.05;
			continue;
		}
		now = gettime();
		if (!isdefined(placed))
			placed = now;
		t = now - placed;
		if (t == 1000 || t == 4300)
		{
			p setWeaponSlotWeapon("satchel", "satchelcharge_mp");
			p setWeaponSlotClipAmmo("satchel", 1);
			p switchToWeapon("satchelcharge_mp");
		}
		if (t == 2500 || t == 4900)
			press(p, 1);
		if (t == 3200 || t == 5600)
			press(p, 0);
		if (t == 4300)
			view = (-10, yaw + turn, 0);
		if (t == 6700 && shoot)
		{
			p setWeaponSlotWeapon("primaryb", "kar98k_mp");
			p setWeaponSlotClipAmmo("primaryb", 5);
			p switchToWeapon("kar98k_mp");
		}
		ms = getentarray("grenade", "classname");
		for (i = 0; i < ms.size; i++)
		{
			n = ms[i] getEntityNumber();
			if (!isdefined(known[n]))
			{
				known[n] = true;
				thrown++;
				logPrint("PROBE throw " + now + " " + n + " " + ms[i].origin + "\n");
				if (thrown == 2)
					target = ms[i];
			}
			logPrint("PROBE miss " + now + " " + n + " " + ms[i].origin + "\n");
		}
		if (distance(p.origin, spot) > 0.5)
			p setorigin(spot);
		if (t >= 6700 && isdefined(target) && shoot)
			view = vectortoangles(target.origin + (0, 0, 3) - (p.origin + (0, 0, 60)));
		p setplayerangles(view);
		if (t == 7600 && shoot)
			press(p, 16);
		if (t == 8300 && shoot)
		{
			press(p, 17);
			if (isdefined(target))
				logPrint("PROBE fire " + now + " " + target getEntityNumber() + " " + target.origin + " " + view + "\n");
		}
		if (t == 8400 && shoot)
			press(p, 16);
		if (t == 8900)
			press(p, 0);
		if (t == 12000)
		{
			logPrint("PROBE done " + now + "\n");
			return;
		}
		wait 0.05;
	}
}
