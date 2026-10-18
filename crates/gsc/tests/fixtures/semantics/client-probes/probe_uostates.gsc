//	The wire round trip's state changes (predict_ab, COD_VERSION=uo
//	tools/run_probe.sh, mp_carentan, a --save-slope --probe-moves client).
//	It puts every allied player on mp_carentan's street at (900 1930) facing
//	world yaw 90, once per spawn, as probe_prone's street spot does, then
//	runs one timeline on the first: a death, a respawn, a spell as a
//	spectator, a respawn, a link to a moving script_origin and the unlink.
//	coduo-movement.md 15.

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
	logPrint("PROBE suicide " + getTime() + "\n");
	self suicide();
	wait 3;
	logPrint("PROBE respawn " + getTime() + "\n");
	self thread maps\mp\gametypes\dm::spawnPlayer();
	wait 6;
	logPrint("PROBE spectate " + getTime() + "\n");
	self thread maps\mp\gametypes\dm::spawnSpectator((900, 1930, 40), (0, 90, 0));
	wait 4;
	logPrint("PROBE respawn " + getTime() + "\n");
	self thread maps\mp\gametypes\dm::spawnPlayer();
	wait 6;
	org = spawn("script_origin", self.origin);
	self linkto(org);
	logPrint("PROBE link " + getTime() + " " + self.origin + "\n");
	org moveto(self.origin + (0, 96, 0), 2);
	wait 4;
	self unlink();
	logPrint("PROBE unlink " + getTime() + " " + self.origin + "\n");
	org delete();
}
