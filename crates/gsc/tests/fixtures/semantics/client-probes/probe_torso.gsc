//	Bones of a player firing and reloading on the move, mp_carentan only.
//	probe_legs' placement (the map's flat bottom floor at (932, -376,
//	-151.875) at level time 25000), with script_origins linked to the
//	spine, arms, hands, weapon tag and feet, so the bones a torso clip keys
//	and the ones it leaves to the legs are both logged. Every server frame
//	for 28 s while the --save-legs --capture-tag torso client fires, reloads
//	and aims through its script:
//
//		PROBE placed <time> <origin>
//		PROBE o <time> <player origin>
//		PROBE b <time> <k> <bone origin> <bone angles>	(k indexes the tags below)
//
//	Run by tools/run_probe.sh with a --save-legs --capture-tag torso client.

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

	tags = [];
	tags[0] = "bip01 pelvis";
	tags[1] = "bip01 spine";
	tags[2] = "bip01 spine2";
	tags[3] = "bip01 neck";
	tags[4] = "bip01 head";
	tags[5] = "bip01 l upperarm";
	tags[6] = "bip01 l forearm";
	tags[7] = "bip01 l hand";
	tags[8] = "bip01 r upperarm";
	tags[9] = "bip01 r forearm";
	tags[10] = "bip01 r hand";
	tags[11] = "tag_weapon_right";
	tags[12] = "bip01 l foot";
	tags[13] = "bip01 r foot";
	marks = [];
	for (j = 0; j < tags.size; j++)
	{
		marks[j] = spawn("script_origin", self.origin);
		marks[j] linkto(self, tags[j], (0, 0, 0), (0, 0, 0));
		logPrint("PROBE tag " + j + " " + tags[j] + " " + marks[j] getEntityNumber() + "\n");
	}

	for (i = 0; i < 560; i++)
	{
		t = getTime();
		logPrint("PROBE o " + t + " " + self.origin + "\n");
		for (j = 0; j < marks.size; j++)
			logPrint("PROBE b " + t + " " + j + " " + marks[j] getorigin() + " " + marks[j].angles + "\n");
		wait 0.05;
	}
	logPrint("PROBE done " + getTime() + "\n");
}
