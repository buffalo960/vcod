//	Does a script's setCvar("sv_hostname") outlive map_restart(false) and a
//	map change? Run by tools/run_probe.sh with
//	+set sv_mapRotation "gametype probe_hostname map mp_brecourt": each pass
//	appends one "x" to sv_hostname and logs how many it found, so the log
//	does not depend on the hostname the server started with.
//	Pass 1 ends in map_restart(false), pass 2 in exitLevel(false), which
//	rotates to mp_brecourt; pass 3 logs and stops.
//	A cvar, not game[], tells the passes apart: exitLevel(false) frees game[].

main()
{
	level.callbackStartGameType = ::Callback_StartGameType;
	level.callbackPlayerConnect = ::Callback_PlayerConnect;
	level.callbackPlayerDisconnect = ::Callback_PlayerDisconnect;
	level.callbackPlayerDamage = ::Callback_PlayerDamage;
	level.callbackPlayerKilled = ::Callback_PlayerKilled;

	maps\mp\gametypes\_callbacksetup::SetupCallbacks();

	pass = getCvar("probe_host_pass");
	if (pass == "")
	{
		setCvar("probe_host_len", "" + getCvar("sv_hostname").size);
		pass = "1";
	}
	found = getCvar("sv_hostname").size - getCvarInt("probe_host_len");
	setCvar("sv_hostname", getCvar("sv_hostname") + "x");
	logPrint("PROBE pass " + pass + " " + getCvar("mapname") + " found " + found + " now " + (found + 1) + "\n");

	if (pass == "1")
	{
		setCvar("probe_host_pass", "2");
		thread end_level_soon(true);
	}
	else if (pass == "2")
	{
		setCvar("probe_host_pass", "3");
		thread end_level_soon(false);
	}
	else
		logPrint("PROBE after_pass " + pass + "\n");
}

end_level_soon(restart)
{
	wait 2;
	if (restart)
		map_restart(false);
	else
		exitLevel(false);
}

Callback_StartGameType() {}

Callback_PlayerConnect() {}
Callback_PlayerDisconnect() {}
Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc) {}
Callback_PlayerKilled(eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc) {}
