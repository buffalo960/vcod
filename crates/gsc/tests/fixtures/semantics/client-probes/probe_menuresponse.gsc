//	What `Cmd_MenuResponse_f` hands the script for each shape of `mr`
//	(docs/research/cod11-gsc-language.md 6). One plain `--net-probe` client
//	sends the lines; every `menuresponse` its entity hears is logged. See
//	this directory's README.md, "probe_menuresponse".

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
	precacheMenu("probe_menu_zero");
}

Callback_PlayerConnect()
{
	self waittill("begin");
	logPrint("PROBE begin " + self getEntityNumber() + "\n");
	for(;;)
	{
		self waittill("menuresponse", menu, response);
		logPrint("PROBE mr [" + menu + "] [" + response + "]\n");
	}
}

Callback_PlayerDisconnect() {}
Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc) {}
Callback_PlayerKilled(eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc) {}
