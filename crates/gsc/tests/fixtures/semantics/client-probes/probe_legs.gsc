//	Bones of a walking player, mp_carentan only. Once an allied player is
//	playing and the level clock reads 25000, puts it on the map's flat bottom
//	floor at (932, -376, -151.875), 145 units under the garden terrain there,
//	links four script_origins to its feet, its right
//	hand and its head, and logs every server frame for 32 s while the
//	--save-legs client walks its script from there:
//
//		PROBE placed <time> <origin>
//		PROBE f <time> <player origin> <l foot origin> <l foot angles> <r foot origin> <r foot angles>
//		PROBE h <time> <hand origin> <hand angles> <head origin> <head angles>
//
//	The mark is an absolute time so that a replay on another server starts
//	the walk on the same level time: a movement clip's first phase is read
//	off it (cod11-movers.md 15).
//	Run by tools/run_probe.sh with a --save-legs client.

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
	for (;;)
	{
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
			players[i] try_run();
		wait 0.05;
	}
}

try_run()
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
	self thread phases();
}

phases()
{
	while (getTime() < 25000)
		wait 0.05;
	self setorigin((932, -376, -151.875));
	logPrint("PROBE player " + self getEntityNumber() + " " + self.model + "\n");
	logPrint("PROBE placed " + getTime() + " " + self.origin + "\n");

	lf = spawn("script_origin", self.origin);
	rf = spawn("script_origin", self.origin);
	hand = spawn("script_origin", self.origin);
	head = spawn("script_origin", self.origin);
	lf linkto(self, "bip01 l foot", (0, 0, 0), (0, 0, 0));
	rf linkto(self, "bip01 r foot", (0, 0, 0), (0, 0, 0));
	hand linkto(self, "tag_weapon_right", (0, 0, 0), (0, 0, 0));
	head linkto(self, "bip01 head", (0, 0, 0), (0, 0, 0));
	logPrint("PROBE ents lf " + lf getEntityNumber() + " rf " + rf getEntityNumber() + " hand " + hand getEntityNumber() + " head " + head getEntityNumber() + "\n");

	for (i = 0; i < 640; i++)
	{
		logPrint("PROBE f " + getTime() + " " + self.origin + " " + lf getorigin() + " " + lf.angles + " " + rf getorigin() + " " + rf.angles + "\n");
		logPrint("PROBE h " + getTime() + " " + hand getorigin() + " " + hand.angles + " " + head getorigin() + " " + head.angles + "\n");
		wait 0.05;
	}
	logPrint("PROBE done " + getTime() + "\n");
}
