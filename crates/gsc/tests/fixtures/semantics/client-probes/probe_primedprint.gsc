//	Which server commands a client gets that were sent while it sat
//	CS_PRIMED, between its gamestate and its first usercmd
//	(`SV_AddServerCommand`, docs/protocol-1.1.md, "The server command
//	queue"). Run with tools/run_probe.sh on mp_carentan, PROBE_SECS 25, and
//	one plain `--net-probe` client behind tmp's hold relay, which drops the
//	client's sequenced packets for a few seconds after the gamestate, so
//	the client stays primed. Every quarter second until `begin` the
//	connect thread sends a self print (type 0), a broadcast print (type 0)
//	and a client cvar (type 1), each numbered; the client's
//	`serverCommand` lines are the measurement.

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
}

Callback_PlayerConnect()
{
	logPrint("PROBE " + getTime() + " connect\n");
	self thread primed();
	self waittill("begin");
	logPrint("PROBE " + getTime() + " begin\n");
	self iprintln("pp entered");
}

primed()
{
	self endon("begin");
	// Few enough type-1 commands that the unacked ring never overflows.
	for (n = 0; n < 24; n++)
	{
		self iprintln("pp self " + n);
		iprintln("pp all " + n);
		self setClientCvar("pp_" + n, n);
		logPrint("PROBE " + getTime() + " sent " + n + "\n");
		wait 0.25;
	}
}

Callback_PlayerDisconnect() {}
Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc) {}
Callback_PlayerKilled(eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc) {}
