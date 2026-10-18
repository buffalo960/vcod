//	When the game hears a client leave: `SV_DropClient` runs the game's
//	`ClientDisconnect`, which notifies `menuresponse` before it runs the
//	callback and frees the entity. Logs the callback's `getTime()`, every
//	`menuresponse` the leaving entity's own thread or a level thread hears,
//	the clock a frame later, and sends one `iPrintLn` a watching client
//	times against the drop's `e` notice. See this directory's README.md,
//	"probe_dropclient".

main()

{
	level.callbackStartGameType = ::Callback_StartGameType;
	level.callbackPlayerConnect = ::Callback_PlayerConnect;
	level.callbackPlayerDisconnect = ::Callback_PlayerDisconnect;
	level.callbackPlayerDamage = ::Callback_PlayerDamage;
	level.callbackPlayerKilled = ::Callback_PlayerKilled;

	maps\mp\gametypes\_callbacksetup::SetupCallbacks();
}


Callback_StartGameType() {}

Callback_PlayerConnect()

{
	self waittill("begin");
	logPrint("PROBE begin " + self getEntityNumber() + " t " + getTime() + "\n");
	self thread watch_menuresponse(self getEntityNumber());
	level thread watch_from_level(self, self getEntityNumber());
}


//	A thread the player does not own; it returns after one wake, so it never
//	waits on a freed entity.
watch_from_level(player, num)

{
	player waittill("menuresponse", menu, response);
	logPrint("PROBE level_mr " + num + " [" + menu + "] [" + response + "] t " + getTime() + "\n");
}


watch_menuresponse(num)

{
	for(;;)
	{
		self waittill("menuresponse", menu, response);
		logPrint("PROBE mr " + num + " [" + menu + "] [" + response + "] t " + getTime() + "\n");
	}
}


Callback_PlayerDisconnect()

{
	num = self getEntityNumber();
	logPrint("PROBE disconnect_callback " + num + " t " + getTime() + "\n");
	iPrintLn("PROBE_GONE " + num);
	level thread after_disconnect(num);
}


after_disconnect(num)

{
	wait 0.05;
	logPrint("PROBE next_frame " + num + " t " + getTime() + "\n");
}


Callback_PlayerDamage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc) {}
Callback_PlayerKilled(eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc) {}
