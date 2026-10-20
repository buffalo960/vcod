//	What a `"` (and `;`, `\`) in a serverinfo cvar does to the serverinfo a
//	client is sent: the gamestate's configstring 0, the `d 0` update and the
//	getinfo/getstatus replies. One plain `--net-probe` client; its `cs 0`
//	lines are the measurement (cod11-server-handshake.md, "The challenge
//	key"). Run with tools/run_probe.sh on mp_carentan, PROBE_SECS 30.

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
	host("q\"h;s\\b e");
	thread later();
}

host(v)
{
	setCvar("sv_hostname", v);
	logPrint("PROBE " + getTime() + " hostname [" + getCvar("sv_hostname") + "]\n");
}

later()
{
	// The client joins inside the first few seconds.
	wait 8;
	host("\"lead");
	wait 3;
	// Nothing left once the three are stripped.
	host("\"");
	wait 3;
	host("tail\"");
	logPrint("PROBE done\n");
}

Callback_PlayerConnect()
{
	logPrint("PROBE " + getTime() + " connect\n");
}

Callback_PlayerDisconnect() {}
Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc) {}
Callback_PlayerKilled(eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc) {}
