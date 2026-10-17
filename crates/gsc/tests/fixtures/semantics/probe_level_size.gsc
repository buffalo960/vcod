//	Isolated: `level.size` after one and two dot-set fields. It reads 1 on
//	retail regardless of how many fields `level` carries: the size opcode
//	answers 1 for any object that is not an array
//	(docs/research/cod11-gsc-language.md section 9).
//	Run by tools/run_probe.sh; every logPrint line is one measurement.

main()
{
	level.callbackStartGameType = ::Callback_StartGameType;
	level.callbackPlayerConnect = ::Callback_PlayerConnect;
	level.callbackPlayerDisconnect = ::Callback_PlayerDisconnect;
	level.callbackPlayerDamage = ::Callback_PlayerDamage;
	level.callbackPlayerKilled = ::Callback_PlayerKilled;

	maps\mp\gametypes\_callbacksetup::SetupCallbacks();

	level.probeFoo = 1;
	logPrint("PROBE level_size_after_one_field " + level.size + "\n");
	level.probeBar = 2;
	logPrint("PROBE level_size_after_two_fields " + level.size + "\n");
}

Callback_StartGameType() {}

Callback_PlayerConnect() {}
Callback_PlayerDisconnect() {}
Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc) {}
Callback_PlayerKilled(eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc) {}
