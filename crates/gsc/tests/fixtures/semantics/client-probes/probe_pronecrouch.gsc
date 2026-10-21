//	A prone press from a crouch, and where a body held a unit up by
//	`setOrigin` comes to rest, alive and dead (docs/research/cod11-mantle.md,
//	"Prone under a setOrigin hold", and
//	cod11-combat.md 5.7). Stock dm on mp_carentan on 1.1d, 1.5b or UO, one
//	`--probe-uo-weapon direct --probe-team allies --probe-weapon
//	m1carbine_mp` client whose `wbuttons` and `up` the script sets through
//	`iPrintLn("PROBE_IN <buttons> <wbuttons> <up>")`.
//
//	The player is set once on probe_legs' flat floor (932 -376 -151.875)
//	and crouches, then holds prone, with `up` 0 (as probe_istouching sent)
//	and again with `up` -127 (as a retail client sends); then he is held
//	there by `setOrigin` every frame for a second, let go and lands. Then
//	the same at the tdm spawn (844 3480), where a body set at -23.75 every
//	frame floats over the ground: the stance keys under the hold, the
//	landing alive, and a body killed in the hold, the keys facing 180 and
//	then 135, and facing 135 on the ground once let go. Each fall logs his origin
//	every frame:
//
//	PROBE phase <time> <name>
//	PROBE pos <time> <x> <y> <z>
//	PROBE done <time>

main()
{
	thread run();
	maps\mp\gametypes\dm::main();
	level.callbackPlayerDamage = ::no_damage;
}

no_damage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc)
{
}

run()
{
	if (getcvar("mapname") != "mp_carentan")
	{
		logPrint("PROBE unsupported\n");
		return;
	}
	player = undefined;
	while (!isdefined(player))
	{
		wait 0.05;
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
			if (isdefined(players[i].sessionstate) && players[i].sessionstate == "playing")
				player = players[i];
	}
	//	The net probe's capture starts 3 s past the weapon pick.
	wait 4;
	spot = (932, -376, -151.875);
	player setorigin(spot);
	player setplayerangles((0, 180, 0));
	player phase("stand", 0, 0, 1.5);
	player phase("crouch", 128, 0, 1.5);
	player phase("prone_up0", 64, 0, 3);
	player phase("stand_again", 0, 0, 2);
	player phase("crouch_down", 128, -127, 1.5);
	player phase("prone_down", 64, -127, 3);
	player phase("stand_last", 0, 0, 2);
	player hold(spot, "hold_alive");
	player fall("land_alive", 1);
	//	The tdm spawn at (844 3480 -8): set every frame at -23.75, the
	//	body floats over the ground there, stance keys held.
	held = (844, 3480, -23.75);
	player thread holding(held);
	player phase("h_stand", 0, 0, 1.5);
	player phase("h_crouch", 128, 0, 1.5);
	player phase("h_prone", 64, 0, 2);
	player phase("h_stand_again", 0, 0, 2);
	player phase("h_prone_stand", 64, 0, 2);
	player phase("h_stand_last", 0, 0, 2);
	//	Facing the spawn's 135, as probe_istouching's first layout did.
	player setplayerangles((0, 135, 0));
	player phase("t_stand", 0, 0, 1);
	player phase("t_crouch", 128, 0, 1.5);
	player phase("t_prone", 64, 0, 2);
	player phase("t_stand_again", 0, 0, 2);
	player phase("t_prone_stand", 64, 0, 2);
	player phase("t_stand_last", 0, 0, 2);
	player notify("probe_release");
	player fall("h_land_alive", 1);
	//	Let go there, on the ground.
	player phase("g_crouch", 128, 0, 1.5);
	player phase("g_prone", 64, 0, 2);
	player phase("g_stand", 0, 0, 2);
	player setplayerangles((0, 180, 0));
	player thread holding(held);
	player phase("h_hold_dead", 0, 0, 1);
	logPrint("PROBE phase " + getTime() + " h_dead\n");
	player suicide();
	player notify("probe_release");
	player fall("h_land_dead", 1.5);
	logPrint("PROBE done " + getTime() + "\n");
}

//	`setOrigin` on every frame until `probe_release`.
holding(spot)
{
	self endon("probe_release");
	for (;;)
	{
		self setorigin(spot);
		wait 0.05;
	}
}

//	`setOrigin` on every frame for a second.
hold(spot, name)
{
	logPrint("PROBE phase " + getTime() + " " + name + "\n");
	for (i = 0; i < 20; i++)
	{
		self setorigin(spot);
		wait 0.05;
	}
	self setorigin(spot);
}

fall(name, secs)
{
	logPrint("PROBE phase " + getTime() + " " + name + "\n");
	for (i = 0; i < secs * 20; i++)
	{
		wait 0.05;
		o = self.origin;
		logPrint("PROBE pos " + getTime() + " " + o[0] + " " + o[1] + " " + o[2] + "\n");
	}
}

phase(name, wb, up, secs)
{
	logPrint("PROBE phase " + getTime() + " " + name + "\n");
	self iprintln("PROBE_IN 0 " + wb + " " + up);
	wait secs;
}
