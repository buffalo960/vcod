//	Threads whose self is an entity that is freed while they wait. One
//	sleeps on `wait` and then calls a method on self (fatal through a dead
//	handle, probe_stale_ent_method), one waits on a level notify sent past
//	the free, and one, on level, holds the entity in a local. The stock
//	jeep self-destruct timer (`_jeepdrive_gmi::process_deactivate`) is the
//	first shape (bot-objectives.md 9.5).
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
	e = spawn("script_origin", (0, 0, 0));
	e.targetname = "stale_self";
	e thread sleeper();
	e thread waiter();
	level thread holder(e);
	e delete();
	wait 0.3;
	level notify("stale_self_go");
	wait 0.1;
	logPrint("PROBE stale_self_after\n");
}

sleeper()
{
	wait 0.2;
	logPrint("PROBE stale_self_sleeper_woke\n");
	n = self getEntityNumber();
	logPrint("PROBE stale_self_sleeper_survived\n");
}

waiter()
{
	level waittill("stale_self_go");
	logPrint("PROBE stale_self_waiter_woke\n");
}

holder(e)
{
	wait 0.2;
	logPrint("PROBE stale_self_holder_woke " + isdefined(e) + "\n");
}

Callback_PlayerConnect() {}
Callback_PlayerDisconnect() {}
Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc) {}
Callback_PlayerKilled(eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc) {}
