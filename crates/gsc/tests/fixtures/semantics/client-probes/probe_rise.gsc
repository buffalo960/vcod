//	The stance refusals' server half (docs/research/cod11-mantle.md, "The
//	forced-stance events"). Stock dm on 1.1d, one `--probe-uo-weapon
//	direct --probe-team allies --probe-weapon m1carbine_mp` client, whose
//	buttons, `wbuttons` and `up` the script sets through
//	`iPrintLn("PROBE_IN <buttons> <wbuttons> <up>")`.
//
//	On mp_carentan the player lies down in the street at (900 1930) facing
//	west, is moved prone under the attic's sloping roof, first where the
//	ceiling clears a 33-high capsule (144 528 288.125: prone fits, crouch
//	does not), then where it clears a 57-high one (176 656 288.125: crouch
//	fits, stand does not), and tries each rise there, with and without
//	`wbuttons` 0x2; then back in the street he stands, holds the trigger
//	and is killed with it held, twice: once through dm's killed callback,
//	which drops his weapon, and, after the use key respawns him, once
//	through a callback that only sets the session `dead`, so the body keeps
//	the weapon. On mp_ship he is put on the hold ladder (4332 -94 120)
//	facing the rungs and holds each stance key. Each phase holds its input:
//
//	PROBE phase <time> <name>
//	PROBE killed <time> <stock|keep>
//	PROBE done <time>

main()
{
	thread run();
	maps\mp\gametypes\dm::main();
	level.probe_killed = level.callbackPlayerKilled;
	level.callbackPlayerKilled = ::probe_killed;
}

probe_killed(eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc)
{
	if (isdefined(level.probe_keep))
	{
		logPrint("PROBE killed " + getTime() + " keep\n");
		self.sessionstate = "dead";
		return;
	}
	logPrint("PROBE killed " + getTime() + " stock\n");
	[[level.probe_killed]](eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc);
}

run()
{
	map = getcvar("mapname");
	if (map != "mp_carentan" && map != "mp_ship")
	{
		logPrint("PROBE unsupported " + map + "\n");
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
	wait 2;
	if (map == "mp_carentan")
		player carentan();
	else
		player ship();
	logPrint("PROBE done " + getTime() + "\n");
}

carentan()
{
	street = (900, 1930, -43);
	low = (144, 528, 288.125);
	mid = (176, 656, 288.125);
	self place(street);
	self phase("street_stand", 0, 0, 1);
	self phase("street_prone", 64, 0, 1.5);
	self place(low);
	self phase("low_prone", 64, 0, 1);
	self phase("low_release", 0, 0, 1);
	self phase("low_crouch", 128, 0, 1);
	self phase("low_crouch_held", 130, 0, 1);
	self phase("low_release_held", 2, 0, 1);
	self phase("low_prone_again", 64, 0, 0.5);
	self place(mid);
	self phase("mid_prone", 64, 0, 1);
	self phase("mid_release", 0, 0, 1);
	self phase("mid_release_held", 2, 0, 1);
	self phase("mid_crouch", 128, 0, 1);
	self phase("mid_lie", 64, 0, 1.5);
	self place(street);
	self phase("street_rise", 0, 0, 1.5);
	self phase("fire", 0, 1, 1);
	logPrint("PROBE phase " + getTime() + " dead\n");
	self suicide();
	wait 1.5;
	//	dm respawns on the use key.
	self input(0, 64);
	while (self.sessionstate != "playing")
		wait 0.05;
	self input(0, 0);
	wait 1;
	self place(street);
	self phase("fire_again", 0, 1, 1);
	level.probe_keep = 1;
	logPrint("PROBE phase " + getTime() + " dead_keep\n");
	self suicide();
	wait 1;
	self input(0, 0);
	wait 0.5;
}

ship()
{
	self place((4332, -94, 120));
	self setplayerangles((0, 90, 0));
	self phase("ladder_none", 0, 0, 1);
	self phase("ladder_crouch", 128, 0, 1);
	self phase("ladder_crouch_held", 130, 0, 1);
	self phase("ladder_prone", 64, 0, 1);
	self phase("ladder_prone_held", 66, 0, 1);
	self phase("ladder_off", 0, 0, 1);
}

place(spot)
{
	self setorigin(spot);
	self setplayerangles((0, 180, 0));
	logPrint("PROBE place " + getTime() + " " + spot + "\n");
}

//	Holds `wb` (and `buttons`) for `secs`, logging the phase's start.
phase(name, wb, buttons, secs)
{
	logPrint("PROBE phase " + getTime() + " " + name + "\n");
	self input(wb, buttons);
	wait secs;
}

input(wb, buttons)
{
	up = 0;
	if (wb & 192)
		up = -127;
	self iprintln("PROBE_IN " + buttons + " " + wb + " " + up);
}
