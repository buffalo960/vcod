//	Whether two notifies of one event in one step both reach a thread that
//	waits on it twice, which value a waiter woken by the first reads, and
//	whether a second event notified right after the first reaches a thread
//	that waits on the first and then the second.
//	Run by tools/run_probe.sh; every logPrint line is one measurement.

main()
{
	level.callbackStartGameType = ::Callback_StartGameType;
	level.callbackPlayerConnect = ::Callback_PlayerConnect;
	level.callbackPlayerDisconnect = ::Callback_PlayerDisconnect;
	level.callbackPlayerDamage = ::Callback_PlayerDamage;
	level.callbackPlayerKilled = ::Callback_PlayerKilled;

	maps\mp\gametypes\_callbacksetup::SetupCallbacks();

	level thread twice();
	level thread once();
	level thread chain();
	wait 0.5;
	level thread pump();
	wait 2;
	logPrint("PROBE notify_twice done\n");
}

Callback_StartGameType() {}

pump()
{
	level notify("tick", 1);
	level notify("tick", 2);
	level notify("first");
	level notify("second");
	logPrint("PROBE notify_twice pumped\n");
}

twice()
{
	level waittill("tick", v);
	logPrint("PROBE notify_twice twice_1 " + v + "\n");
	level waittill("tick", v);
	logPrint("PROBE notify_twice twice_2 " + v + "\n");
}

once()
{
	level waittill("tick", v);
	logPrint("PROBE notify_twice once " + v + "\n");
}

chain()
{
	level waittill("first");
	logPrint("PROBE notify_twice chain_first\n");
	level waittill("second");
	logPrint("PROBE notify_twice chain_second\n");
}

Callback_PlayerConnect() {}
Callback_PlayerDisconnect() {}
Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc) {}
Callback_PlayerKilled(eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc) {}
