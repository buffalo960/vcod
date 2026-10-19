//	The states the client predicts besides a live body, on 1.1d (predict_ab,
//	tools/run_probe.sh, mp_carentan, a --save-slope --slope-every-state
//	--probe-moves client). It puts every allied player on mp_carentan's
//	street at (900 1930) facing world yaw 90, once per spawn, as
//	probe_uostates does, then runs one timeline on the first, timed against
//	the moves script: a 22 s spell as a free spectator over the street
//	(its runs, the held jump's climb and the crouch's dive onto the
//	ground), a respawn, a suicide inside the diagonal runs (a corpse that
//	slides) and the intermission. cod11-player-clip.md 1.1.
//	+set probe_suicide_wait <s> moves the suicide (14 s after the respawn by
//	default); 10.5 kills 1.5b's body inside the stance timer (pm_time).

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
			players[i] try_place();
		wait 0.05;
	}
}

try_place()
{
	if (!isdefined(self.sessionstate))
		return;
	if (self.sessionstate != "playing")
	{
		self.probe_placed = undefined;
		return;
	}
	if (self.pers["team"] != "allies")
		return;
	if (isdefined(self.probe_placed))
		return;
	self.probe_placed = 1;
	self setorigin((900, 1930, -38));
	self setplayerangles((0, 90, 0));
	logPrint("PROBE place " + getTime() + " " + self getEntityNumber() + "\n");
	if (!isdefined(self.probe_timeline))
	{
		self.probe_timeline = 1;
		self thread timeline();
	}
}

timeline()
{
	self endon("disconnect");
	wait 8;
	logPrint("PROBE spectate " + getTime() + "\n");
	self thread maps\mp\gametypes\dm::spawnSpectator((900, 1930, 40), (0, 90, 0));
	wait 22;
	logPrint("PROBE respawn " + getTime() + "\n");
	self thread maps\mp\gametypes\dm::spawnPlayer();
	suicide_wait = 14;
	if (getCvar("probe_suicide_wait") != "")
		suicide_wait = getCvarFloat("probe_suicide_wait");
	wait suicide_wait;
	logPrint("PROBE suicide " + getTime() + " " + self.origin + "\n");
	self suicide();
	wait 4;
	logPrint("PROBE intermission " + getTime() + "\n");
	self thread maps\mp\gametypes\dm::spawnIntermission();
}
