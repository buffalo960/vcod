//	Whether a conditional takes a numeric string: UO's stock tdm.gsc tests
//	`if (getCvar("scr_teamscorepenalty"))`, which its own setup leaves at
//	"1", on every suicide and team kill. probe_truthy and
//	probe_truthy_empty measured "a" and "" fatal.
//	Run by tools/run_probe.sh; every logPrint line is one measurement.

main()
{
	level.callbackStartGameType = ::Callback_StartGameType;
	level.callbackPlayerConnect = ::Callback_PlayerConnect;
	level.callbackPlayerDisconnect = ::Callback_PlayerDisconnect;
	level.callbackPlayerDamage = ::Callback_PlayerDamage;
	level.callbackPlayerKilled = ::Callback_PlayerKilled;

	maps\mp\gametypes\_callbacksetup::SetupCallbacks();

	logPrint("PROBE at truthy_str_one\n");
	if ("1")
		logPrint("PROBE truthy_str_one 1\n");
	else
		logPrint("PROBE truthy_str_one 0\n");
	logPrint("PROBE at truthy_str_zero\n");
	if ("0")
		logPrint("PROBE truthy_str_zero 1\n");
	else
		logPrint("PROBE truthy_str_zero 0\n");
}

Callback_StartGameType() {}

Callback_PlayerConnect() {}
Callback_PlayerDisconnect() {}
Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc) {}
Callback_PlayerKilled(eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc) {}
