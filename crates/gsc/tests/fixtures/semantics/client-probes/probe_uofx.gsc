//	United Offensive's own events on the wire (docs/research/coduo-diff.md
//	section 8), for the retail UO 1.51b server only. The first player to
//	spawn gets the weapon the `probe_weapon` cvar names (none: it keeps
//	its own) and a switch to it, which a client following the server's
//	`a` selects takes; a second on the script raises an earthquake and a
//	rewindFx on it, each logged as `PROBE <step> <time>`:
//
//	weapon  the weapon handed over
//	quake   earthquake(0.4, 1.5, the player's origin, 600)
//	rewind  rewindFx(player, 1), which sends that client `F 1000`
//
//	A binoculars weapon goes into the binocular slot with one strike
//	loaded, as `_rank_gmi`'s rank 4 gets it, the smoke grenade into its
//	own slot with three, the satchel charge into its own with one.
//
//	The shooter is `vcod --net-probe --probe-team axis --save-combat
//	--capture-tag <t>` (its own weapon), `--probe-weapsel` (which takes
//	the switch and pulls the trigger from 15 s) or `--probe-uo-weapon
//	<script>` (the artillery, smoke and bipod scripts); a second `--probe-team
//	spectator --probe-follow --save-snapshots --capture-tag <t>` records
//	both (recipe in docs/research/coduo-diff.md section 19). The cvar is
//	read at the spawn, so `tools/run_probe.sh`'s `+set` after the map
//	reaches it.

main()
{
	// dm precaches none of these; a weapon handed over unprecached
	// aborts the server.
	precacheItem("flamethrower_mp");
	precacheItem("binoculars_artillery_mp");
	precacheItem("smokegrenade_mp");
	precacheItem("mg34_mp");
	precacheItem("satchelcharge_mp");
	thread watch();
	maps\mp\gametypes\dm::main();
}

watch()
{
	for (;;)
	{
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
		{
			if (isdefined(players[i].sessionstate) && players[i].sessionstate == "playing")
			{
				players[i] run();
				return;
			}
		}
		wait 0.05;
	}
}

run()
{
	weapon = getCvar("probe_weapon");
	if (weapon == "binoculars_artillery_mp" || weapon == "binoculars_mp")
	{
		// _rank_gmi's rank-4 grant: the binocular slot, one strike loaded.
		self setWeaponSlotWeapon("binocular", weapon);
		self setWeaponSlotClipAmmo("binocular", 1);
	}
	else if (weapon == "smokegrenade_mp")
	{
		// As _teams::giveSmokeGrenades; a giveWeapon of it aborts the server.
		self setWeaponSlotWeapon("smokegrenade", weapon);
		self setWeaponSlotClipAmmo("smokegrenade", 3);
	}
	else if (weapon == "satchelcharge_mp")
	{
		self setWeaponSlotWeapon("satchel", weapon);
		self setWeaponSlotClipAmmo("satchel", 1);
	}
	else if (weapon != "")
	{
		self giveWeapon(weapon);
		self giveMaxAmmo(weapon);
	}
	// A select sent with the spawn reaches the client before its first
	// playing snapshot, and is dropped.
	wait 1;
	if (weapon != "")
	{
		self switchToWeapon(weapon);
		step("weapon " + weapon);
	}
	step("quake");
	earthquake(0.4, 1.5, self.origin, 600);
	wait 2;
	step("rewind");
	rewindfx(self, 1);
}

step(name)
{
	logPrint("PROBE " + name + " " + getTime() + "\n");
}
