//	Where placeSpawnpoint puts every spawnpoint a stock gametype drops, to the
//	float's last bit: each line carries the height as %g and as its sign, its
//	whole part and its fraction in 2^-24ths (exact for 1 <= |z| < 65536), and
//	when bits remain past those (|z| < 1) the next 24 as a fourth number,
//	built by comparisons since the language has no integer cast. A
//	`PROBE class` line heads each spawn class the map carries.
//	Run: tools/run_probe.sh client-probes/probe_placebits mp_carentan
//	(COD_VERSION=1.5 or uo for the other servers).

main()
{
	level.callbackStartGameType = ::Callback_StartGameType;
	level.callbackPlayerConnect = ::Callback_PlayerConnect;
	level.callbackPlayerDisconnect = ::Callback_PlayerDisconnect;
	level.callbackPlayerDamage = ::Callback_PlayerDamage;
	level.callbackPlayerKilled = ::Callback_PlayerKilled;

	maps\mp\gametypes\_callbacksetup::SetupCallbacks();
}

Callback_StartGameType()
{
	// Team deathmatch first: the mp_carentan capture this extends logged it alone.
	classes = [];
	classes[classes.size] = "mp_teamdeathmatch_spawn";
	classes[classes.size] = "mp_deathmatch_spawn";
	classes[classes.size] = "mp_searchanddestroy_spawn_allied";
	classes[classes.size] = "mp_searchanddestroy_spawn_axis";
	classes[classes.size] = "mp_retrieval_spawn_allied";
	classes[classes.size] = "mp_retrieval_spawn_axis";
	classes[classes.size] = "mp_uo_spawn_allies";
	classes[classes.size] = "mp_uo_spawn_axis";
	classes[classes.size] = "mp_uo_spawn_allies_secondary";
	classes[classes.size] = "mp_uo_spawn_axis_secondary";
	classes[classes.size] = "mp_gmi_bas_allies_spawn";
	classes[classes.size] = "mp_gmi_bas_axis_spawn";
	classes[classes.size] = "mp_gmi_bas_allied_spawn_secondary";
	classes[classes.size] = "mp_gmi_bas_axis_spawn_secondary";
	classes[classes.size] = "mp_gmi_bas_allied_secondary_spawn";
	classes[classes.size] = "mp_gmi_bas_axis_secondary_spawn";
	for (c = 0; c < classes.size; c++)
	{
		spawnpoints = getentarray(classes[c], "classname");
		if (!spawnpoints.size)
			continue;
		logPrint("PROBE class " + classes[c] + " " + spawnpoints.size + "\n");
		for (i = 0; i < spawnpoints.size; i++)
		{
			spawnpoints[i] placeSpawnpoint();
			o = spawnpoints[i].origin;
			logPrint("PROBE placebits " + spawnpoints[i] getEntityNumber() + " " + o[0] + " " + o[1] + " " + o[2] + " " + bits(o[2]) + "\n");
		}
	}
}

bits(z)
{
	sign = "+";
	a = z;
	if (z < 0)
	{
		sign = "-";
		a = 0 - z;
	}
	whole = 0;
	p = 32768;
	while (p >= 1)
	{
		if (whole + p <= a)
			whole = whole + p;
		p = p / 2;
	}
	f = a - whole;
	frac = 0;
	for (k = 0; k < 24; k++)
	{
		f = f * 2;
		frac = frac * 2;
		if (f >= 1)
		{
			f = f - 1;
			frac = frac + 1;
		}
	}
	if (f == 0)
		return sign + " " + whole + " " + frac;
	rest = 0;
	for (k = 0; k < 24; k++)
	{
		f = f * 2;
		rest = rest * 2;
		if (f >= 1)
		{
			f = f - 1;
			rest = rest + 1;
		}
	}
	return sign + " " + whole + " " + frac + " " + rest;
}

Callback_PlayerConnect() {}
Callback_PlayerDisconnect() {}
Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc) {}
Callback_PlayerKilled(eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc) {}
