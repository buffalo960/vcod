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
//	He is put there twice, a second apart, so the second drop starts at
//	rest and the airborne prone body it leaves (cod15-movement.md 4)
//	repeats.
//
//	Spots: `brecourt` (mp_brecourt) is probe_scriptmodel's prone target,
//	96 out from the site, facing back at 270; `mound` (mp_carentan) is
//	probe_prone's ~19 degree terrain mound at 270; `flat` (mp_carentan) is
//	probe_scriptmodel's prone target there, at 225. `probe_hold 1` puts him
//	back whenever he is more than half a unit off the spot, z included, as
//	probe_scriptmodel's prone part did before 2026-10-16.
//
//	`probe_spawn_yaw <yaw>` opens the first placement with a `spawn` on the
//	spot at that yaw (PROBE spawn <time> <entity number> <spot> <yaw>),
//	the view turned to the spot's yaw on the same frame: 1.5's and UO's
//	spawn think runs the client's last cmd, so a client holding prone lies
//	down along the spawn's yaw before the turn (cod15-movement.md 4, "The
//	spawn's lie-down").
//
//	`probe_rays 1` adds, after the bones, two frames of the body's origin
//	and each bone mark printed to the float (each component as its integer
//	part and the exact remainder), and a grid of downward `bulletTrace`s
//	with `hitCharacters` over the body, 2 units apart, each that stops on
//	him logged with where it stopped in z, likewise:
//
//	PROBE xbody <time> <entity number> <x> <y> <z>
//	PROBE xbone <time> <entity number> <tag> <x> <y> <z>
//	PROBE ray <time> <dx> <dy> <z>
//
//	(<dx> <dy> the ray's offset off the spot.)

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
	self thread place(spot);
}

//	Placed twice, a second apart: the first placement keeps whatever
//	velocity the spawn left, and the second drops a body at rest.
place(spot)
{
	self endon("disconnect");
	for (i = 0; i < 2; i++)
	{
		if (i)
			wait 1;
		if (self.sessionstate != "playing")
			return;
		if (!i && getcvar("probe_spawn_yaw") != "")
		{
			yaw = getcvarfloat("probe_spawn_yaw");
			self spawn(spot.origin, (0, yaw, 0));
			logPrint("PROBE spawn " + getTime() + " " + self getEntityNumber() + " " + spot.origin + " " + yaw + "\n");
		}
		self setorigin(spot.origin);
		self setplayerangles((0, spot.yaw, 0));
		logPrint("PROBE place " + getTime() + " " + self getEntityNumber() + " " + spot.origin + " " + spot.yaw + "\n");
	}
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
	if (getcvarint("probe_rays"))
		log_rays(n, tags, marks);
	logPrint("PROBE done " + getTime() + "\n");
}

//	A float as its integer part and the remainder, which the subtraction
//	keeps exact and `%g` prints to below the float's step. gsc has no
//	`int`: the cvar's atoi of the printed float truncates it.
exact(f)
{
	setcvar("probe_int", "" + f);
	i = getcvarint("probe_int");
	return i + " " + (f - i);
}

exact3(v)
{
	return exact(v[0]) + " " + exact(v[1]) + " " + exact(v[2]);
}

log_rays(n, tags, marks)
{
	spot = spot_of(getcvar("probe_spot"));
	for (k = 0; k < 2; k++)
	{
		wait 0.05;
		logPrint("PROBE xbody " + getTime() + " " + n + " " + exact3(self.origin) + "\n");
		for (j = 0; j < tags.size; j++)
			logPrint("PROBE xbone " + getTime() + " " + n + " " + tags[j] + " " + exact3(marks[j].origin) + "\n");
		top = self.origin[2] + 40;
		bottom = self.origin[2] - 30;
		for (dx = -16; dx <= 16; dx += 2)
		{
			for (dy = -40; dy <= 70; dy += 2)
			{
				x = spot.origin[0] + dx;
				y = spot.origin[1] + dy;
				tr = bulletTrace((x, y, top), (x, y, bottom), true, undefined);
				if (isdefined(tr["entity"]) && tr["entity"] == self)
					logPrint("PROBE ray " + getTime() + " " + dx + " " + dy + " " + exact(tr["position"][2]) + "\n");
			}
		}
	}
}
