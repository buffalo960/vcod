//	What a runtime error does on a server below `developer 1`: UO's stock
//	tdm.gsc `Callback_PlayerKilled` tests `if (level.teamkill_penalty)`,
//	which nothing sets, on every death to something not a player (line
//	815). Run it as
//	  PRE_MAP_ARGS='+set developer 0' COD_VERSION=uo tools/run_probe.sh client-probes/probe_undef_if mp_cassino
//	and again with `developer 1` (run_probe.sh's own) for the fatal case;
//	on UO add `+set probe_builtin 0` after the map (see below).
//	Every logPrint line is one measurement; no client joins.

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
	thread run();
	thread other();
}

run()
{
	wait 1;
	logPrint("PROBE developer " + getCvar("developer") + "\n");
	x = 5;
	x--;
	logPrint("PROBE before_if " + x + "\n");
	if (level.teamkill_penalty)
	{
		logPrint("PROBE if_taken\n");
		x = 100;
	}
	else
		logPrint("PROBE if_else\n");
	logPrint("PROBE after_if " + x + "\n");
	// The callback's tail: a string built after the error.
	y = "K;" + x + ";" + "world";
	logPrint("PROBE tail " + y + "\n");
	if (!level.teamkill_penalty)
		logPrint("PROBE not_taken\n");
	else
		logPrint("PROBE not_else\n");
	logPrint("PROBE after_not\n");
	z = level.unset_number + 1;
	logPrint("PROBE after_add " + isDefined(z) + "\n");
	n = !level.unset_number;
	logPrint("PROBE not_value " + isDefined(n) + "\n");
	f = level.unset_struct.field;
	logPrint("PROBE field_value " + isDefined(f) + "\n");
	// A builtin's own error: UO 1.51b aborts the process on it at
	// developer 0, so `+set probe_builtin 0` skips it there.
	if (getCvar("probe_builtin") != "0")
	{
		e = distance(level.unset_number, (0, 0, 0));
		logPrint("PROBE builtin_value " + isDefined(e) + "\n");
	}
	t = getTime();
	wait level.unset_number;
	logPrint("PROBE wait_took " + (getTime() - t) + "\n");
	wait 0.5;
	logPrint("PROBE thread_alive\n");
}

// Another thread, to tell a dead thread from a dead server.
other()
{
	wait 3;
	logPrint("PROBE other_alive\n");
}

Callback_PlayerConnect() {}
Callback_PlayerDisconnect() {}
Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc) {}
Callback_PlayerKilled(eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc) {}
