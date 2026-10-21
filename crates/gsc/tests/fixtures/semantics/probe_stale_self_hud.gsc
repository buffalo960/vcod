//	A thread whose self is a hudelem destroyed while it waits, then
//	writes a field on self (fatal through a dead handle,
//	probe_stale_hud_write). probe_stale_self is the entity case.
//	Run by tools/run_probe.sh; every logPrint line is one measurement.

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
	h = newHudElem();
	h thread sleeper();
	h destroy();
	wait 0.3;
	logPrint("PROBE stale_self_hud_after\n");
}

sleeper()
{
	wait 0.2;
	logPrint("PROBE stale_self_hud_woke\n");
	self.alpha = 1;
	logPrint("PROBE stale_self_hud_survived\n");
}

Callback_PlayerConnect() {}
Callback_PlayerDisconnect() {}
Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc) {}
Callback_PlayerKilled(eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc) {}
