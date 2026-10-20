//	What `G_MoverPush`'s candidate trace (game.mp 0x554ee) lets through
//	besides the pusher (movers doc 12, "The candidate trace"). mp_carentan,
//	dm rules, two clients. Keeps the bombzone_A brush model *5 (probe_ride's
//	slab) and runs one-frame movey's into players set down in its path:
//	two stacked on one spot, two overlapping by half a box, one sunk into
//	the floor, and one standing clear as the control. Every frame of a push
//	logs the slab's origin and both players'. A third client connected as a spectator keeps
//	`StuckInClient` off (player-clip doc 6.2), so the stacked pair keeps its
//	BODY contents and stays put. Recipe: README.md.

main()
{
	bms = getentarray("script_brushmodel", "classname");
	for (i = 0; i < bms.size; i++)
		bms[i].script_gameobjectname = "dm";
	thread drive();
	maps\mp\gametypes\dm::main();
}

state(players, tag)
{
	logPrint("PROBE " + tag + " bz " + level.bz.origin + "\n");
	for (i = 0; i < players.size; i++)
		logPrint("PROBE " + tag + " " + players[i] getEntityNumber() + " " + players[i].origin + "\n");
}

// Both players off the slab's path, then the slab back where it started.
clear(players, bz, dy)
{
	players[0] setorigin((-215, 2330, -31));
	players[1] setorigin((-300, 2330, -31));
	wait 0.5;
	bz movey(dy, 0.05);
	wait 1;
	state(players, "cleared");
}

push(players, bz, tag, dy)
{
	wait 1;
	state(players, "set_" + tag);
	logPrint("PROBE push " + tag + " " + getTime() + "\n");
	bz movey(dy, 0.05);
	for (i = 0; i < 4; i++)
	{
		state(players, "f_" + tag + "_" + getTime());
		wait 0.05;
	}
	wait 0.5;
	state(players, "after_" + tag);
	clear(players, bz, 0 - dy);
}

drive()
{
	bms = getentarray("script_brushmodel", "classname");
	bz = undefined;
	for (i = 0; i < bms.size; i++)
	{
		if (!isdefined(bz) || bms[i] getEntityNumber() < bz getEntityNumber())
			bz = bms[i];
	}
	for (;;)
	{
		players = getentarray("player", "classname");
		n = 0;
		for (i = 0; i < players.size; i++)
		{
			if (isdefined(players[i].sessionstate) && players[i].sessionstate == "playing")
				n++;
		}
		if (n >= 2)
			break;
		wait 0.05;
	}
	wait 2;
	level.bz = bz;
	logPrint("PROBE bz " + bz getEntityNumber() + "\n");
	all = getentarray("player", "classname");
	players = [];
	for (i = 0; i < all.size; i++)
	{
		if (isdefined(all[i].sessionstate) && all[i].sessionstate == "playing")
			players[players.size] = all[i];
	}
	for (i = 0; i < players.size; i++)
		players[i].health = 1000;

	players[0] setorigin((-215, 2430, -31));
	players[1] setorigin((-300, 2330, -31));
	push(players, bz, "alone", -8);

	players[0] setorigin((-215, 2430, -31));
	players[1] setorigin((-215, 2430, -31));
	push(players, bz, "stack", -8);

	players[1] setorigin((-200, 2430, -31));
	players[0] setorigin((-215, 2430, -31));
	push(players, bz, "half", -8);

	players[0] setorigin((-215, 2430, -34));
	players[1] setorigin((-300, 2330, -31));
	push(players, bz, "sunk", -8);

	wait 1;
	logPrint("PROBE done\n");
}
