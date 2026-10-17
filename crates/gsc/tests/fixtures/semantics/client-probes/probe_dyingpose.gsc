//	A dying body's bones and its corpse's event ring
//	(docs/research/cod11-combat.md 16.4, "The dying body", and 5.2).
//	dm with its killed callback replaced. The first player to spawn is put
//	on mp_carentan's flat prone spot facing `probe_yaw` (default 90) and
//	stands still. Once settled, a script_origin is linked to each of eight
//	bones; every frame for 0.5 s alive and 2.5 s after a `suicide()` the
//	script logs his origin, angles and the marks:
//
//	PROBE place <time> <entity number> <origin> <yaw>
//	PROBE body <time> <entity number> <origin> <angles> <sessionstate>
//	PROBE bone <time> <entity number> <tag> <origin>
//	PROBE killed <time> <entity number> body <corpse entity number>
//
//	The killed callback takes the dead session, then clones the corpse and
//	plays an alias on it, the way a mod's death sound would: the corpse
//	rides its own snapshot with that event in its ring.

main()
{
	thread watch_players();
	maps\mp\gametypes\dm::main();
	level.callbackPlayerKilled = ::Callback_PlayerKilled;
}

watch_players()
{
	wait 1;
	for (;;)
	{
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
		{
			p = players[i];
			if (!isdefined(level.probe_done) && isdefined(p.sessionstate) && p.sessionstate == "playing")
			{
				level.probe_done = 1;
				p thread run();
			}
		}
		wait 0.05;
	}
}

log_frame(n, tags, marks)
{
	logPrint("PROBE body " + getTime() + " " + n + " " + self.origin + " " + self.angles + " " + self.sessionstate + "\n");
	for (j = 0; j < tags.size; j++)
		logPrint("PROBE bone " + getTime() + " " + n + " " + tags[j] + " " + marks[j].origin + "\n");
}

run()
{
	self endon("disconnect");
	n = self getEntityNumber();
	yaw = 90;
	if (getcvar("probe_yaw") != "")
		yaw = getcvarfloat("probe_yaw");
	spot = (1435.88, 2675.88, -23.75);
	self setorigin(spot);
	self setplayerangles((0, yaw, 0));
	logPrint("PROBE place " + getTime() + " " + n + " " + spot + " " + yaw + "\n");
	tags = [];
	tags[0] = "bip01 pelvis";
	tags[1] = "bip01 spine2";
	tags[2] = "bip01 neck";
	tags[3] = "bip01 head";
	tags[4] = "bip01 l hand";
	tags[5] = "bip01 r hand";
	tags[6] = "bip01 l foot";
	tags[7] = "bip01 r foot";
	wait 3;
	marks = [];
	for (j = 0; j < tags.size; j++)
	{
		marks[j] = spawn("script_origin", self.origin);
		marks[j] linkto(self, tags[j], (0, 0, 0), (0, 0, 0));
	}
	wait 0.1;
	for (k = 0; k < 10; k++)
	{
		self log_frame(n, tags, marks);
		wait 0.05;
	}
	self suicide();
	for (k = 0; k < 50; k++)
	{
		self log_frame(n, tags, marks);
		wait 0.05;
	}
	logPrint("PROBE done " + getTime() + "\n");
}

Callback_PlayerKilled(eInflictor, attacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc)
{
	self.sessionstate = "dead";
	body = self cloneplayer();
	b = -1;
	if (isdefined(body))
	{
		b = body getEntityNumber();
		body playsound("MP_bomb_plant");
	}
	logPrint("PROBE killed " + getTime() + " " + self getEntityNumber() + " body " + b + "\n");
}
