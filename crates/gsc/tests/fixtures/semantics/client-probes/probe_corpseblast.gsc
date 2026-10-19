//	A death in the entity pass: the corpse's 0x800 clock when the killing
//	blast is a grenade's (docs/research/cod11-combat.md 5.2).
//	dm with its killed callback wrapped. The first player to spawn is put on
//	mp_carentan's flat prone spot facing 90 and looking down, handed a frag
//	and left on 1 health; a `--probe-uo-weapon throw` client drops it at his
//	feet and its blast kills him. The callback logs the clock it runs on
//	before dm's own clones the corpse:
//
//	PROBE place <time> <entity number> <origin>
//	PROBE killed <time> <entity number> <means of death>
//
//	The corpse's eFlags edges ride the client's snapshots (`corpse 64`
//	lines and the --save-snapshots pair).

main()
{
	thread watch_players();
	maps\mp\gametypes\dm::main();
	precacheItem("fraggrenade_mp");
	level.callbackPlayerKilled = ::Callback_PlayerKilled;
}

watch_players()
{
	wait 1;
	for (;;)
	{
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
		{
			p = players[i];
			if (!isdefined(level.probe_done) && isdefined(p.sessionstate) && p.sessionstate == "playing")
			{
				level.probe_done = 1;
				p thread run();
			}
		}
		wait 0.05;
	}
}

run()
{
	self endon("disconnect");
	spot = (1435.88, 2675.88, -23.75);
	self setorigin(spot);
	logPrint("PROBE place " + getTime() + " " + self getEntityNumber() + " " + spot + "\n");
	self setWeaponSlotWeapon("grenade", "fraggrenade_mp");
	self giveMaxAmmo("fraggrenade_mp");
	self switchToWeapon("fraggrenade_mp");
	self.health = 1;
	while (self.sessionstate == "playing")
	{
		self setplayerangles((70, 90, 0));
		wait 0.05;
	}
	wait 3;
	logPrint("PROBE done " + getTime() + "\n");
}

Callback_PlayerKilled(eInflictor, attacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc)
{
	logPrint("PROBE killed " + getTime() + " " + self getEntityNumber() + " " + sMeansOfDeath + "\n");
	maps\mp\gametypes\dm::Callback_PlayerKilled(eInflictor, attacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc);
}
