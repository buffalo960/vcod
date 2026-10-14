//	A thread started right after a notify, with an endon on that notify
//	(UO hq.gsc's `notify("Timer Changed"); thread hq_reinforcement_timer()`),
//	and one that waits on it.
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

	level.ticks = 0;
	level thread ticker();
	wait 0.2;
	level notify("probe_changed");
	level thread ticker();
	wait 0.5;
	logPrint("PROBE endon_after_notify " + level.ticks + "\n");
	level.woke = 0;
	level notify("probe_again");
	level thread waiter();
	wait 0.2;
	logPrint("PROBE waittill_after_notify " + level.woke + "\n");
}

Callback_StartGameType() {}

waiter()
{
	level waittill("probe_again");
	level.woke = 1;
}

ticker()
{
	level endon("probe_changed");
	for (;;)
	{
		wait 0.05;
		level.ticks++;
	}
}

Callback_PlayerConnect() {}
Callback_PlayerDisconnect() {}
Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc) {}
Callback_PlayerKilled(eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc) {}
