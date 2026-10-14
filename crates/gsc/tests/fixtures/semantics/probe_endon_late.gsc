//	Whether a notify kills a thread that registers its `endon` after the
//	notify, in the same step: UO's stock gametypes' `checkMatchStart` does
//	`level notify("kill_endround")` and then `level thread endRound(...)`,
//	whose first line is `level endon("kill_endround")`. Also whether a thread
//	spawned before the notify in the same step is killed, and whether a
//	waittill registered after the notify is woken by it.
//	Run by tools/run_probe.sh; every logPrint line is one measurement.

main()
{
	level.callbackStartGameType = ::Callback_StartGameType;
	level.callbackPlayerConnect = ::Callback_PlayerConnect;
	level.callbackPlayerDisconnect = ::Callback_PlayerDisconnect;
	level.callbackPlayerDamage = ::Callback_PlayerDamage;
	level.callbackPlayerKilled = ::Callback_PlayerKilled;

	maps\mp\gametypes\_callbacksetup::SetupCallbacks();

	level thread notifier();
	wait 3;
	logPrint("PROBE endon_late done " + 1 + "\n");
}

Callback_StartGameType() {}

notifier()
{
	level thread early();
	level notify("probe_kill");
	level thread late();
	level thread late_wait();
	logPrint("PROBE endon_late notified\n");
}

early()
{
	level endon("probe_kill");
	wait 1;
	logPrint("PROBE endon_late early_survived\n");
}

late()
{
	level endon("probe_kill");
	logPrint("PROBE endon_late late_armed\n");
	wait 1;
	logPrint("PROBE endon_late late_survived\n");
}

late_wait()
{
	level waittill("probe_kill");
	logPrint("PROBE endon_late late_wait_woken\n");
}

Callback_PlayerConnect() {}
Callback_PlayerDisconnect() {}
Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc) {}
Callback_PlayerKilled(eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc) {}
