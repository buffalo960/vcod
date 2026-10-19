//	Where placeSpawnpoint puts every team deathmatch spawn of the map, to the
//	float's last bit: each line carries the height as %g and as its sign, its
//	whole part and its fraction in 2^-24ths (exact for 1 <= |z| < 65536),
//	built by comparisons since the language has no integer cast.
//	Run: tools/run_probe.sh client-probes/probe_placebits mp_carentan
//	(COD_VERSION=uo for United Offensive).

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
	spawnpoints = getentarray("mp_teamdeathmatch_spawn", "classname");
	for (i = 0; i < spawnpoints.size; i++)
	{
		spawnpoints[i] placeSpawnpoint();
		o = spawnpoints[i].origin;
		logPrint("PROBE placebits " + spawnpoints[i] getEntityNumber() + " " + o[0] + " " + o[1] + " " + o[2] + " " + bits(o[2]) + "\n");
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
	return sign + " " + whole + " " + frac;
}

Callback_PlayerConnect() {}
Callback_PlayerDisconnect() {}
Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc) {}
Callback_PlayerKilled(eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc) {}
