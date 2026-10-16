//	Whether a spectator's spawn think runs the client's last cmd
//	(cod15-movement.md 4, "The spawn's lie-down"). Every 2 s from 3 s in,
//	each spectator is spawned again as a spectator on a spot in the air
//	over mp_carentan's mound, facing 0, and his origin is logged straight
//	after the `spawn` returns and on the next frame:
//
//	PROBE spec <time> <entity number> <origin after the spawn>
//	PROBE next <time> <entity number> <origin a frame later>
//
//	A `--probe-edge spec-fly` client holds forward throughout: a think on
//	his last cmd flies him off the spot inside the spawn, a null cmd
//	leaves him on it.

main()
{
	thread watch_players();
	maps\mp\gametypes\dm::main();
}

watch_players()
{
	wait 3;
	for (;;)
	{
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
			players[i] try_spawn();
		wait 2;
	}
}

//	Early returns: sessionstate is undefined before the first spawn.
try_spawn()
{
	if (!isdefined(self.sessionstate))
		return;
	if (self.sessionstate != "spectator")
		return;
	self thread respawn();
}

respawn()
{
	self endon("disconnect");
	self spawn((-224, 60, 200), (0, 0, 0));
	logPrint("PROBE spec " + getTime() + " " + self getEntityNumber() + " " + self.origin + "\n");
	wait 0.05;
	logPrint("PROBE next " + getTime() + " " + self getEntityNumber() + " " + self.origin + "\n");
}
