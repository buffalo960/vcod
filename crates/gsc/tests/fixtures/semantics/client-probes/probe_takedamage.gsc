//	A script model that takes damage (`setTakeDamage`, United Offensive
//	only) struck by a rocket and by a swing (docs/research/cod11-combat.md
//	2.5, 13.1). Stock tdm. The first player to spawn is the shooter, held
//	on the first tdm spawn point past the first with 256 units of clear
//	level ground on its facing, its view turned every frame onto the middle
//	of an `xmodel/barrel_black1` script model with `setTakeDamage(1)` that
//	stands `probe_dist` units out (default 200). A second in it is handed
//	the weapon `probe_weapon` names (`bazooka_mp`, into the second primary
//	slot; none keeps its own), empty, with a round into its clip 4, 10
//	and 16 s after, so each leaves fully sighted. Every `damage` notify
//	the barrel takes is logged:
//
//	PROBE site <time> <spot> <yaw> <barrel number> <barrel origin>
//	PROBE mdamage <time> <amount> <mod> <point>
//
//	rocket: `--probe-uo-weapon aim` with `bazooka_mp`, the barrel 200 out.
//	melee: `--probe-uo-weapon melee`, `+set probe_dist 40`.
//
//	COD_VERSION=uo PORT=32502 PROBE_SECS=50 tools/run_probe.sh \
//	    client-probes/probe_takedamage mp_carentan +set probe_weapon bazooka_mp
//	vcod --net-probe 127.0.0.1:32502 --game-dir <uo install> --mod-dir uo \
//	    --probe-team allies --probe-uo-weapon aim --probe-secs 30

main()
{
	precacheModel("xmodel/barrel_black1");
	w = getcvar("probe_weapon");
	if (w != "")
		precacheItem(w);
	thread run(w);
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
		if (isdefined(p.sessionstate) && p.sessionstate == "playing")
			return p;
	}
	return undefined;
}

log_damage()
{
	for (;;)
	{
		self waittill("damage", amount, attacker, dir, point, mod);
		logPrint("PROBE mdamage " + gettime() + " " + amount + " " + mod + " " + point + "\n");
	}
}

run(w)
{
	wait 1;
	dist = 200;
	if (getcvar("probe_dist") != "")
		dist = getcvarint("probe_dist");
	spawns = getentarray("mp_teamdeathmatch_spawn", "classname");
	if (!spawns.size)
		spawns = getentarray("mp_tdm_spawn", "classname");
	spot = undefined;
	for (i = 1; i < spawns.size && !isdefined(spot); i++)
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
		spot = a + (0, 0, 0.125);
	}
	fwd = anglestoforward((0, yaw, 0));
	model = spawn("script_model", ground_at(spot + (fwd[0] * dist, fwd[1] * dist, 0)));
	model setmodel("xmodel/barrel_black1");
	model setTakeDamage(1);
	model thread log_damage();
	logPrint("PROBE site " + gettime() + " " + spot + " " + yaw + " " + model getEntityNumber() + " " + model.origin + "\n");
	at = model.origin + (0, 0, 20);
	placed = undefined;
	for (;;)
	{
		shooter = first_player();
		if (isdefined(shooter))
		{
			now = gettime();
			if (!isdefined(placed))
				placed = now;
			if (now == placed + 1000 && w != "")
			{
				shooter setWeaponSlotWeapon("primaryb", w);
				shooter setWeaponSlotAmmo("primaryb", 0);
				shooter setWeaponSlotClipAmmo("primaryb", 0);
				shooter switchToWeapon(w);
			}
			if (w != "" && (now == placed + 5000 || now == placed + 11000 || now == placed + 17000))
				shooter setWeaponSlotClipAmmo("primaryb", 1);
			if (distance(shooter.origin, spot) > 0.5)
				shooter setorigin(spot);
			shooter setplayerangles(vectortoangles(at - (spot + (0, 0, 60))));
		}
		wait 0.05;
	}
}
