//	Isolated: `self["k"] = 1`. Retail types `self`, `level` and `anim` as
//	objects when it compiles, as probe_level_bracket.gsc measured for
//	`level`; a compile error voids the whole file, so it sits alone.
//	Run by tools/run_probe.sh; every logPrint line is one measurement.

main()
{
	level.callbackStartGameType = ::Callback_StartGameType;
	level.callbackPlayerConnect = ::Callback_PlayerConnect;
	level.callbackPlayerDisconnect = ::Callback_PlayerDisconnect;
	level.callbackPlayerDamage = ::Callback_PlayerDamage;
	level.callbackPlayerKilled = ::Callback_PlayerKilled;

	maps\mp\gametypes\_callbacksetup::SetupCallbacks();

	logPrint("PROBE at self_bracket_write\n");
	self["k"] = 1;
	logPrint("PROBE self_bracket_write_ok 1\n");
	logPrint("PROBE self_bracket_read " + self["k"] + "\n");
}

Callback_StartGameType() {}

Callback_PlayerConnect() {}
Callback_PlayerDisconnect() {}
Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc) {}
Callback_PlayerKilled(eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc) {}
