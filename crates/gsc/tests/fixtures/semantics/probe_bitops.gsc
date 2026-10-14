//	The bitwise operators: |, ^ and &, their precedence, and ^ on a string
//	(UO's dom.gsc has `", "^7` in an iPrintLn argument list).
//	Run by tools/run_probe.sh; every logPrint line is one measurement.
//	Groups are separate files because a script runtime error takes the whole
//	retail server down, so one fatal expression would cost every measurement
//	after it.

main()
{
	level.callbackStartGameType = ::Callback_StartGameType;
	level.callbackPlayerConnect = ::Callback_PlayerConnect;
	level.callbackPlayerDisconnect = ::Callback_PlayerDisconnect;
	level.callbackPlayerDamage = ::Callback_PlayerDamage;
	level.callbackPlayerKilled = ::Callback_PlayerKilled;

	maps\mp\gametypes\_callbacksetup::SetupCallbacks();

	logPrint("PROBE xor " + (5 ^ 3) + " " + (0 - 1 ^ 1) + "\n");
	logPrint("PROBE or " + (5 | 3) + "\n");
	logPrint("PROBE or_xor " + (1 | 2 ^ 3) + "\n");
	logPrint("PROBE xor_and " + (6 ^ 3 & 5) + "\n");
	logPrint("PROBE xor_eq " + (2 ^ 3 == 1) + "\n");
	logPrint("PROBE add_xor " + (1 + 2 ^ 3) + "\n");
	logPrint("PROBE and_andand " + (1 && 2 & 1) + "\n");
	logPrint("PROBE at xor_string\n");
	logPrint("PROBE xor_string " + (", " ^ 7) + "\n");
}

Callback_StartGameType() {}

Callback_PlayerConnect() {}
Callback_PlayerDisconnect() {}
Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc) {}
Callback_PlayerKilled(eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc) {}
