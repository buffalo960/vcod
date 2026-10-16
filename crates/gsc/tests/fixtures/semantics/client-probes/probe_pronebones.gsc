//	A prone body's bones on a grade (docs/research/player-model-anim-system.md,
//	"The prone body on a grade"). Every live player is put once per spawn on
//	the spot `probe_spot` names, facing its yaw, and a `--probe-fall
//	--probe-fall-prone <yaw>` client lies down there. His origin is logged
//	every frame for 3 s; then, each frame for 2 s, his origin and a
//	script_origin linked to each of eight bones (a link reads the pose a
//	frame late, so the body has to lie still):
//
//	PROBE place <time> <entity number> <spot> <yaw>
//	PROBE pos <time> <entity number> <origin>
//	PROBE bone <time> <entity number> <tag> <origin>
//	PROBE body <time> <entity number> <origin>
//
//	Spots: `brecourt` (mp_brecourt) is probe_scriptmodel's prone target,
//	96 out from the site, facing back at 270; `mound` (mp_carentan) is
//	probe_prone's ~19 degree terrain mound at 270; `flat` (mp_carentan) is
//	probe_scriptmodel's prone target there, at 225. `probe_hold 1` puts him
//	back whenever he is more than half a unit off the spot, z included, as
//	probe_scriptmodel's prone part did before 2026-10-16.

main()
{
	thread watch_players();
	maps\mp\gametypes\dm::main();
}

spot_of(name)
{
	s = spawnstruct();
	if (name == "brecourt")
	{
		s.origin = (1659, 781, -102.76);
		s.yaw = 270;
	}
	else if (name == "mound")
	{
		s.origin = (-224, 60, 45);
		s.yaw = 270;
	}
	else
	{
		s.origin = (1435.88, 2675.88, -23.75);
		s.yaw = 225;
	}
	return s;
}

watch_players()
{
	wait 1;
	spot = spot_of(getcvar("probe_spot"));
	for (;;)
	{
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
			players[i] try_place(spot);
		wait 0.05;
	}
}

//	Early returns: sessionstate is undefined before the first spawn.
try_place(spot)
{
	if (!isdefined(self.sessionstate))
		return;
	if (self.sessionstate != "playing")
	{
		self.probe_placed = undefined;
		return;
	}
	if (isdefined(self.probe_placed))
		return;
	self.probe_placed = 1;
	self setorigin(spot.origin);
	self setplayerangles((0, spot.yaw, 0));
	logPrint("PROBE place " + getTime() + " " + self getEntityNumber() + " " + spot.origin + " " + spot.yaw + "\n");
	self thread log_bones();
	if (getcvarint("probe_hold"))
		self thread hold(spot);
}

//	probe_scriptmodel's hold on its target: put back whenever he is more
//	than half a unit off the spot, z included.
hold(spot)
{
	self endon("disconnect");
	for (;;)
	{
		wait 0.05;
		if (self.sessionstate != "playing")
			return;
		if (distance(self.origin, spot.origin) > 0.5)
			self setorigin(spot.origin);
	}
}

log_bones()
{
	self endon("disconnect");
	tags = [];
	tags[0] = "bip01 pelvis";
	tags[1] = "bip01 spine2";
	tags[2] = "bip01 neck";
	tags[3] = "bip01 head";
	tags[4] = "bip01 l hand";
	tags[5] = "bip01 r hand";
	tags[6] = "bip01 l foot";
	tags[7] = "bip01 r foot";
	//	Linked after the wait: the spawn sets the body's model later.
	n = self getEntityNumber();
	for (k = 0; k < 60; k++)
	{
		logPrint("PROBE pos " + getTime() + " " + n + " " + self.origin + "\n");
		wait 0.05;
	}
	marks = [];
	for (j = 0; j < tags.size; j++)
	{
		marks[j] = spawn("script_origin", self.origin);
		marks[j] linkto(self, tags[j], (0, 0, 0), (0, 0, 0));
	}
	wait 0.1;
	for (k = 0; k < 40; k++)
	{
		logPrint("PROBE body " + getTime() + " " + n + " " + self.origin + "\n");
		for (j = 0; j < tags.size; j++)
			logPrint("PROBE bone " + getTime() + " " + n + " " + tags[j] + " " + marks[j].origin + "\n");
		wait 0.05;
	}
	logPrint("PROBE done " + getTime() + "\n");
}
