//	What an int spawn key reads when its value is no number: mp_foy's base
//	walls carry "script_idnumber" "foo" (the axis hq's are targetname
//	kw_10), and bas.gsc deletes a breached base's walls only when
//	isdefined(script_idnumber). Run alone on the retail UO 1.51b server:
//	COD_VERSION=uo tools/run_probe.sh client-probes/probe_spawnint mp_foy
//	(no client). Measurements: docs/research/bot-objectives.md 7.

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
	ents = getentarray("kw_10", "targetname");
	logPrint("PROBE count " + ents.size + "\n");
	for (i = 0; i < ents.size; i++)
	{
		e = ents[i];
		if (isdefined(e.script_idnumber))
			logPrint("PROBE ent " + e.classname + " idnumber " + e.script_idnumber + "\n");
		else
			logPrint("PROBE ent " + e.classname + " idnumber undefined\n");
	}
}

Callback_PlayerConnect() {}
Callback_PlayerDisconnect() {}
Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc) {}
Callback_PlayerKilled(eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc) {}
