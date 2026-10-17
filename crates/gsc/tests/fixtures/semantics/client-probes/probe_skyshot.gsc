//	Missiles and swings at the edges (docs/research/cod11-combat.md 2.5,
//	12.3), any patch. Stock tdm. The first player to spawn is the shooter,
//	held where the part puts it every frame, its view set every frame (a
//	`--probe-uo-weapon aim|throw|melee` probe sends the view the gsc set).
//
//	sky: on the first tdm spawn point whose view 80 degrees up along its
//	facing meets the sky (the height the highest upward trace from a
//	spawn point stops at), looking along it, and handed the weapon
//	`probe_weapon` names a second in (`panzerfaust_mp` or `bazooka_mp`
//	into the second primary slot, `fraggrenade_mp` into the grenade
//	slot), with `--probe-uo-weapon aim` for a launcher or `throw` for the
//	frag. It comes empty, one round goes into the clip 4, 10 and 16 s
//	after the handover, so each leaves fully sighted (and UO's
//	`setWeaponSlotWeapon` and `giveMaxAmmo` fill no clip where 1.1's do). Every missile alive is logged every frame, by classname:
//
//	PROBE sky <time> <spot> <sky z> <surfacetype>
//	PROBE miss <time> <entity number> <classname> <origin>
//
//	air: on the first tdm spawn point past the first with 128 units of
//	clear level ground on its facing, looking along it (or `probe_pitch`
//	degrees down, 80 for the floor); a `--probe-uo-weapon melee` probe
//	swings at nothing.
//
//	PROBE air <time> <spot> <yaw> <pitch>
//
//	spot: at (`probe_x`, `probe_y`, `probe_z`), the ground's height, looking
//	along `probe_yaw` and `probe_pitch` (mp_harbor's water, combat doc
//	12.1), handed the weapon as the sky part does.
//
//	PROBE spot <time> <spot> <yaw> <pitch>
//
//	A player's damage callback only logs and takes no health:
//
//	PROBE hit <time> <entity number> dmg <amount> mod <mod>
//
//	Recipe (retail 1.1d; `COD_VERSION=uo` and `bazooka_mp` for UO):
//
//	PORT=32501 PROBE_SECS=60 tools/run_probe.sh client-probes/probe_skyshot \
//	    mp_carentan +set probe_part sky +set probe_weapon panzerfaust_mp
//	vcod --net-probe 127.0.0.1:32501 --probe-team allies \
//	    --probe-uo-weapon aim --probe-secs 30

main()
{
	w = getcvar("probe_weapon");
	if (w != "")
		precacheItem(w);
	thread run(getcvar("probe_part"), w);
	maps\mp\gametypes\tdm::main();
	level.callbackPlayerDamage = ::probe_damage;
}

probe_damage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc)
{
	logPrint("PROBE hit " + gettime() + " " + self getEntityNumber() + " dmg " + iDamage + " mod " + sMeansOfDeath + "\n");
}

spawn_points()
{
	spawns = getentarray("mp_teamdeathmatch_spawn", "classname");
	if (!spawns.size)
		spawns = getentarray("mp_tdm_spawn", "classname");
	return spawns;
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
		if (isdefined(p.sessionstate) && p.sessionstate == "playing")
			return p;
	}
	return undefined;
}

log_missiles(name)
{
	ms = getentarray(name, "classname");
	for (i = 0; i < ms.size; i++)
		logPrint("PROBE miss " + gettime() + " " + ms[i] getEntityNumber() + " " + name + " " + ms[i].origin + "\n");
}

run(part, w)
{
	wait 1;
	spawns = spawn_points();
	spot = undefined;
	yaw = 0;
	pitch = 0;
	if (part == "sky")
	{
		//	The sky's height: the highest an upward trace from a spawn point
		//	stops. Then the first spawn point whose aim, 80 degrees up along
		//	its facing, stops within a unit of it.
		top = -100000;
		for (i = 0; i < spawns.size; i++)
		{
			o = spawns[i].origin;
			tr = bullettrace(o + (0, 0, 60), o + (0, 0, 16000), false, undefined);
			if (tr["fraction"] < 1 && tr["position"][2] > top)
				top = tr["position"][2];
		}
		pitch = -80;
		for (i = 0; i < spawns.size && !isdefined(spot); i++)
		{
			a = ground_at(spawns[i].origin + (0, 0, 8));
			yaw = spawns[i].angles[1];
			fwd = anglestoforward((pitch, yaw, 0));
			tr = bullettrace(a + (0, 0, 60), a + (0, 0, 60) + (fwd[0] * 16000, fwd[1] * 16000, fwd[2] * 16000), false, undefined);
			if (tr["fraction"] < 1 && tr["position"][2] > top - 1)
			{
				spot = a + (0, 0, 0.125);
				surf = tr["surfacetype"];
			}
		}
		logPrint("PROBE sky " + gettime() + " " + spot + " " + top + " " + surf + "\n");
	}
	else if (part == "spot")
	{
		spot = (getcvarfloat("probe_x"), getcvarfloat("probe_y"), getcvarfloat("probe_z") + 0.125);
		yaw = getcvarfloat("probe_yaw");
		pitch = getcvarfloat("probe_pitch");
		logPrint("PROBE spot " + gettime() + " " + spot + " " + yaw + " " + pitch + "\n");
	}
	else
	{
		for (i = 1; i < spawns.size && !isdefined(spot); i++)
		{
			yaw = spawns[i].angles[1];
			fwd = anglestoforward((0, yaw, 0));
			a = ground_at(spawns[i].origin + (0, 0, 8));
			b = ground_at(a + (fwd[0] * 128, fwd[1] * 128, 0));
			dz = b[2] - a[2];
			if (dz > 2 || dz < -2)
				continue;
			if (bullettrace(a + (0, 0, 60), b + (0, 0, 60), false, undefined)["fraction"] < 1)
				continue;
			spot = a + (0, 0, 0.125);
		}
		if (getcvar("probe_pitch") != "")
			pitch = getcvarfloat("probe_pitch");
		logPrint("PROBE air " + gettime() + " " + spot + " " + yaw + " " + pitch + "\n");
	}
	placed = undefined;
	for (;;)
	{
		shooter = first_player();
		if (isdefined(shooter))
		{
			now = gettime();
			if (!isdefined(placed))
				placed = now;
			slot = "primaryb";
			if (w == "fraggrenade_mp")
				slot = "grenade";
			if (w == "smokegrenade_mp")
				slot = "smokegrenade";
			if (now == placed + 1000 && w != "")
			{
				shooter setWeaponSlotWeapon(slot, w);
				shooter setWeaponSlotAmmo(slot, 0);
				shooter setWeaponSlotClipAmmo(slot, 0);
				shooter switchToWeapon(w);
			}
			if (w != "" && (now == placed + 5000 || now == placed + 11000 || now == placed + 17000))
				shooter setWeaponSlotClipAmmo(slot, 1);
			log_missiles("rocket");
			log_missiles("grenade");
			if (distance(shooter.origin, spot) > 0.5)
				shooter setorigin(spot);
			shooter setplayerangles((pitch, yaw, 0));
		}
		wait 0.05;
	}
}
