//	A linked corpse (pm_type 7) on 1.1d, for predict_ab (tools/run_probe.sh,
//	mp_carentan, a --save-slope --slope-every-state --probe-moves client).
//	It puts every allied player on mp_carentan's street at (900 1930) facing
//	world yaw 90, once per spawn, as probe_states does, then times one
//	timeline on the first against the moves script: inside the diagonal runs
//	it links him to a script_origin where he stands, kills him half a second
//	later and leaves the corpse linked until the moves script's first shot
//	respawns him (ClientSpawn unlinks). cod11-player-clip.md 1.1.
//	+set probe_link_wait <s> moves the link (43.8 s after the placement by
//	default).

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
	link_wait = 43.8;
	if (getCvar("probe_link_wait") != "")
		link_wait = getCvarFloat("probe_link_wait");
	wait link_wait;
	anchor = spawn("script_origin", self.origin);
	self linkto(anchor);
	logPrint("PROBE link " + getTime() + " " + self.origin + "\n");
	wait 0.5;
	logPrint("PROBE suicide " + getTime() + " " + self.origin + "\n");
	self suicide();
	for (i = 0; i < 200; i++)
	{
		logPrint("PROBE dead " + getTime() + " " + self.sessionstate + " " + self.origin + "\n");
		if (self.sessionstate == "playing")
			break;
		wait 0.05;
	}
}
