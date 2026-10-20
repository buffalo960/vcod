//	Pickup_Weapon's draw for an item with no count, on mp_carentan with one
//	allied player. The frag's dropAmmoMin and dropAmmoMax are both 0, so a
//	script-spawned frag hands out `1 + trunc((clipSize - 1) * f + 0.5)`
//	grenades, f the module's draw (items doc 4.1). Twenty times: empty the
//	grenade slot's clip, spawn an `mpweapon_fraggrenade` at the feet, and log
//	the clip the walk-on pickup left. Run by tools/run_probe.sh with a
//	--probe-items --probe-team allies client.

main()
{
	thread watch_players();
	maps\mp\gametypes\dm::main();
}

watch_players()
{
	if (getcvar("mapname") != "mp_carentan")
	{
		logPrint("PROBE unsupported " + getcvar("mapname") + "\n");
		return;
	}
	wait 1;
	for (;;)
	{
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
			players[i] try_start();
		wait 0.05;
	}
}

try_start()
{
	if (!isdefined(self.sessionstate))
		return;
	if (self.sessionstate != "playing")
		return;
	if (isdefined(self.probe_running))
		return;
	if (self.pers["team"] != "allies")
		return;
	self.probe_running = 1;
	self thread run();
}

run()
{
	flat = (838, 2222, -23);
	away = (900, 1930, -42);

	wait 3;
	logPrint("PROBE start " + getTime() + " " + self getWeaponSlotWeapon("grenade") + "\n");
	for (i = 0; i < 20; i++)
	{
		self setorigin(away);
		self setWeaponSlotClipAmmo("grenade", 0);
		wait 0.5;
		f = spawn("mpweapon_fraggrenade", flat + (0, 0, 1));
		self setorigin(flat);
		f waittill("trigger");
		wait 0.05;
		logPrint("PROBE frag " + i + " " + getTime() + " " + self getWeaponSlotClipAmmo("grenade") + "\n");
	}
	logPrint("PROBE done " + getTime() + "\n");
}
