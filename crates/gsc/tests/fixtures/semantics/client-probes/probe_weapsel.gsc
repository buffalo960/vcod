//	Weapon selection capture's server half. On each allied player's first
//	spawn it waits 4 s, has switchToWeapon ask for the pistol (the engine only
//	sends the client `a <weapon>`), 4 s on asks for the primary back, 4 s on
//	empties the primary's clip and reserve, and 8 s on logs what is held.
//	Run by tools/run_probe.sh with a --probe-weapsel client, which pulls the
//	trigger from 15 s after its join settles.

main()
{
	thread watch_players();
	maps\mp\gametypes\dm::main();
}

watch_players()
{
	wait 1;
	for (;;)
	{
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
			players[i] try_start();
		wait 0.05;
	}
}

//	Early returns rather than one compound test: sessionstate is undefined
//	before the first spawn.
try_start()
{
	if (!isdefined(self.sessionstate))
		return;
	if (self.sessionstate != "playing")
		return;
	if (isdefined(self.probe_started))
		return;
	if (self.pers["team"] != "allies")
		return;
	self.probe_started = 1;
	self thread run();
}

held(what)
{
	logPrint("PROBE " + what + " " + getTime() + " current " + self getcurrentweapon() + "\n");
}

run()
{
	self endon("disconnect");
	primary = self getweaponslotweapon("primary");
	pistol = self getweaponslotweapon("pistol");
	logPrint("PROBE weapons " + getTime() + " primary " + primary + " pistol " + pistol + "\n");
	wait 4;
	self held("before_pistol");
	if (pistol != "none")
		self switchToWeapon(pistol);
	wait 4;
	self held("before_primary");
	self switchToWeapon(primary);
	wait 4;
	self held("before_dry");
	self setweaponslotclipammo("primary", 0);
	self setweaponslotammo("primary", 0);
	wait 8;
	self held("after_dry");
}
