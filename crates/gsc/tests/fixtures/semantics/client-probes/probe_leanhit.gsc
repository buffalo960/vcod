//	A leaning target's hit pose and a leaning shooter's rounds
//	(docs/research/cod11-combat.md 2.1, 3, 16.4). Stock tdm, any patch.
//
//	Two `--probe-uo-weapon direct` clients: an allies shooter
//	(`--probe-weapon springfield_mp`) and an axis target. The site is the
//	first tdm spawn point past the first with 256 units of level ground
//	clear on its facing (probe_scriptmodel's pair site): the shooter
//	stands on it, the target 256 ahead facing back, both held there and
//	their views set every frame. The script drives both clients' inputs
//	through `iPrintLn("PROBE_IN <buttons> <wbuttons> <up>")`, so every
//	phase starts on the server's clock. Once both play, the target lies
//	prone and stands again, which sets his legs and swings his torso onto
//	his view whichever spawn point tdm gave him; the site has 80 units of
//	level ground clear behind him for the prone body:
//
//	PROBE site <time> <shooter spot> <target spot> <yaw>
//	PROBE phase <time> <name>
//
//	Each phase holds its stance and lean, then a grid of level
//	`bulletTrace`s (hitCharacters on) crosses the target: `f` rows run
//	along the shooter's facing from 64 in front of the target to 80
//	behind, 11 across from 30 left to 30 right; `s` rows run across from
//	48 right of him to 48 left, 15 along from 30 in front to 54 behind.
//	A row is a height off the target's feet, 4 to 76 every 6. Each cell
//	is the distance from the ray's start to where it stopped on the
//	target, or `-` when it stopped elsewhere or ran on:
//
//	PROBE grid <time> <phase> <f|s> <height> <cell>...
//
//	Then the shooter, sighted all along, taps the trigger once, aimed
//	from his own leaned eye (the 19.2 across and 5.5 down of a full lean)
//	at the target's chest moved half his lean. The damage callback logs
//	and takes no health:
//
//	PROBE hit <time> <phase> <team> dmg <amount> loc <location> point <point>
//
//	Before the phases the target's attachments, the head and the gear
//	random draws whose boxes differ (cod11-turrets.md 13.6):
//
//	PROBE models <entity number> <model>...
//
//	`probe_bones 1` also links a script_origin to each of eleven of the
//	target's bones 0.35 s before each grid and logs his origin and them on
//	three frames, each as its offset from the target's spot (a link reads the pose a
//	frame late, so the body has settled by then); the marks go before the
//	grid's frame:
//
//	PROBE body <time> <phase> <dx> <dy> <dz>
//	PROBE bone <time> <phase> <tag> <dx> <dy> <dz>
//	PROBE eye <time> <phase> <dx> <dy> <dz>
//
//	`eye` is the shooter's origin off his spot. `probe_fall 1` logs each
//	player's height off his spot for ten frames after every `setOrigin`
//	the hold makes, the drop of a unit and a bit it starts:
//
//	PROBE fall <time> <shooter|target> <dz>
//	PROBE spawnat <time> <shooter|target> <x> <y> <z> <pitch> <yaw> <roll>
//	PROBE dir <time> <phase> <x> <y> <z>
//
//	`spawnat`, under `probe_fall`, is the tdm spawn point nearest the
//	player at his first `setOrigin`, which a replay pins: a sloped one
//	leaves the spawn think's velocity on him through the drop. `dir` is
//	each round's direction, after its `hit`.
//
//	`probe_pin 1` puts a fixed head and gear on the target before the
//	phases (the character script draws them at random), and after the
//	level phase's grid takes his attachments off and puts the same ones
//	back, then runs the grid again in that frame as phase `level_re`.
//	`probe_stamp 1` crosses the target with one `bulletTrace` right after
//	the second `bones` row, in that thread (movers doc 15).

main()
{
	//	`probe_pin`'s models, which the axis character scripts precache.
	precacheModel("xmodel/basehead_axis_a_01");
	precacheModel("xmodel/gear_german_helmet_falshrm_blk");
	precacheModel("xmodel/gear_german_load1_falshrm");
	precacheModel("xmodel/gear_german_bandolier");
	thread run();
	maps\mp\gametypes\tdm::main();
	level.callbackPlayerDamage = ::probe_damage;
}

probe_damage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc)
{
	who = self getEntityNumber();
	if (isdefined(self.pers["team"]))
		who = self.pers["team"];
	logPrint("PROBE hit " + gettime() + " " + level.probe_phase + " " + who + " dmg " + iDamage + " loc " + sHitLoc + " point " + vPoint + "\n");
	if (getcvarint("probe_fall"))
		logPrint("PROBE dir " + gettime() + " " + level.probe_phase + " " + vDir[0] + " " + vDir[1] + " " + vDir[2] + "\n");
}

ground_at(p)
{
	tr = bullettrace(p + (0, 0, 64), p + (0, 0, -128), false, undefined);
	return tr["position"];
}

//	probe_scriptmodel's pick_site, with 80 units of level ground clear at
//	8 and 20 up past the target's spot.
pick_site()
{
	spawns = getentarray("mp_teamdeathmatch_spawn", "classname");
	if (!spawns.size)
		spawns = getentarray("mp_tdm_spawn", "classname");
	for (i = 1; i < spawns.size; i++)
	{
		yaw = spawns[i].angles[1];
		fwd = anglestoforward((0, yaw, 0));
		a = ground_at(spawns[i].origin + (0, 0, 8));
		b = ground_at(a + (fwd[0] * 256, fwd[1] * 256, 0));
		dz = b[2] - a[2];
		if (dz > 2 || dz < -2)
			continue;
		if (bullettrace(a + (0, 0, 60), b + (0, 0, 60), false, undefined)["fraction"] < 1)
			continue;
		if (bullettrace(a + (0, 0, 20), b + (0, 0, 20), false, undefined)["fraction"] < 1)
			continue;
		mid = ground_at(a + (fwd[0] * 128, fwd[1] * 128, 0));
		dz = mid[2] - a[2];
		if (dz > 2 || dz < -2)
			continue;
		c = ground_at(b + (fwd[0] * 80, fwd[1] * 80, 0));
		dz = c[2] - a[2];
		if (dz > 2 || dz < -2)
			continue;
		if (bullettrace(b + (0, 0, 8), c + (0, 0, 8), false, undefined)["fraction"] < 1)
			continue;
		if (bullettrace(b + (0, 0, 20), c + (0, 0, 20), false, undefined)["fraction"] < 1)
			continue;
		site = spawnstruct();
		site.a = a + (0, 0, 0.125);
		site.b = b + (0, 0, 0.125);
		site.yaw = yaw;
		site.fwd = fwd;
		return site;
	}
	return undefined;
}

team_player(team)
{
	players = getentarray("player", "classname");
	for (i = 0; i < players.size; i++)
	{
		p = players[i];
		if (isdefined(p.sessionstate) && p.sessionstate == "playing" && isdefined(p.pers["team"]) && p.pers["team"] == team)
			return p;
	}
	return undefined;
}

//	One phase: target and shooter `wbuttons` (0x10 lean left, 0x20 lean
//	right, 0x40 prone, 0x80 crouch) and how long it settles.
phase(name, twb, swb, settle)
{
	p = spawnstruct();
	p.name = name;
	p.twb = twb;
	p.swb = swb;
	p.settle = settle;
	return p;
}

phases()
{
	ps = [];
	ps[ps.size] = phase("level", 0, 0, 1.5);
	ps[ps.size] = phase("t_left", 16, 0, 1.5);
	ps[ps.size] = phase("t_right", 32, 0, 1.5);
	ps[ps.size] = phase("t_crouch", 128, 0, 1.5);
	ps[ps.size] = phase("t_crouch_left", 144, 0, 1.5);
	ps[ps.size] = phase("t_crouch_right", 160, 0, 1.5);
	ps[ps.size] = phase("t_prone", 64, 0, 2.5);
	ps[ps.size] = phase("t_prone_left", 80, 0, 1.5);
	ps[ps.size] = phase("t_prone_right", 96, 0, 1.5);
	ps[ps.size] = phase("t_stand", 0, 0, 2.5);
	ps[ps.size] = phase("s_left", 0, 16, 1.5);
	ps[ps.size] = phase("s_right", 0, 32, 1.5);
	ps[ps.size] = phase("s_left_t_right", 32, 16, 1.5);
	return ps;
}

//	`up` -127 under crouch or prone, as a client holds it.
send_in(player, buttons, wb)
{
	up = 0;
	if (wb & 192)
		up = -127;
	player iprintln("PROBE_IN " + buttons + " " + wb + " " + up);
}

//	The eye offset of a full lean `wb` along `yaw` (AddLeanToPosition).
lean_off(wb, yaw)
{
	s = 0;
	if (wb & 16)
		s = -1;
	if (wb & 32)
		s = 1;
	if (s == 0)
		return (0, 0, 0);
	r = anglestoright((0, yaw, 0));
	return (r[0] * 19.225 * s, r[1] * 19.225 * s, -5.513);
}

bones(name, site, target, delay)
{
	wait delay;
	tags = [];
	tags[0] = "bip01 pelvis";
	tags[1] = "bip01 spine2";
	tags[2] = "bip01 neck";
	tags[3] = "bip01 head";
	tags[4] = "bip01 l upperarm";
	tags[5] = "bip01 r upperarm";
	tags[6] = "bip01 l forearm";
	tags[7] = "bip01 r forearm";
	tags[8] = "bip01 l hand";
	tags[9] = "bip01 r hand";
	tags[10] = "tag_weapon_right";
	marks = [];
	for (j = 0; j < tags.size; j++)
	{
		marks[j] = spawn("script_origin", target.origin);
		marks[j] linkto(target, tags[j], (0, 0, 0), (0, 0, 0));
	}
	wait 0.1;
	for (k = 0; k < 3; k++)
	{
		d = target.origin - site.b;
		logPrint("PROBE body " + gettime() + " " + name + " " + d[0] + " " + d[1] + " " + d[2] + "\n");
		shooter = team_player("allies");
		if (isdefined(shooter))
		{
			d = shooter.origin - site.a;
			logPrint("PROBE eye " + gettime() + " " + name + " " + d[0] + " " + d[1] + " " + d[2] + "\n");
		}
		for (j = 0; j < tags.size; j++)
		{
			d = marks[j].origin - site.b;
			logPrint("PROBE bone " + gettime() + " " + name + " " + tags[j] + " " + d[0] + " " + d[1] + " " + d[2] + "\n");
		}
		if (k == 1 && getcvarint("probe_stamp"))
			bullettrace(site.b + (0, 0, 40) - (site.fwd[0] * 64, site.fwd[1] * 64, 0), site.b + (0, 0, 40) + (site.fwd[0] * 80, site.fwd[1] * 80, 0), true, undefined);
		wait 0.05;
	}
	for (j = 0; j < marks.size; j++)
		marks[j] delete();
}

//	Takes every attachment off and puts `models` back on the same tags, or
//	the ones the target wears when `models` is undefined.
reattach(models)
{
	n = self getattachsize();
	tags = [];
	have = [];
	for (i = 0; i < n; i++)
	{
		tags[i] = self getattachtagname(i);
		have[i] = self getattachmodelname(i);
	}
	if (!isdefined(models))
		models = have;
	self detachall();
	for (i = 0; i < n; i++)
	{
		if (tags[i] == "")
			self attach(models[i]);
		else
			self attach(models[i], tags[i]);
	}
}

log_models()
{
	line = "PROBE models " + self getEntityNumber();
	n = self getattachsize();
	for (i = 0; i < n; i++)
		line = line + " " + self getattachmodelname(i);
	logPrint(line + "\n");
}

cell(start, end, target)
{
	tr = bullettrace(start, end, true, undefined);
	if (!isdefined(tr["entity"]) || tr["entity"] != target || tr["fraction"] >= 1)
		return "-";
	return "" + tr["fraction"] * distance(start, end);
}

grid(name, site, target)
{
	fwd = site.fwd;
	r = anglestoright((0, site.yaw, 0));
	o = site.b;
	for (h = 4; h <= 76; h += 6)
	{
		line = "PROBE grid " + gettime() + " " + name + " f " + h;
		for (x = -30; x <= 30; x += 6)
		{
			c = o + (r[0] * x, r[1] * x, h);
			line = line + " " + cell(c - (fwd[0] * 64, fwd[1] * 64, 0), c + (fwd[0] * 80, fwd[1] * 80, 0), target);
		}
		logPrint(line + "\n");
	}
	for (h = 4; h <= 76; h += 6)
	{
		line = "PROBE grid " + gettime() + " " + name + " s " + h;
		for (y = -30; y <= 54; y += 6)
		{
			c = o + (fwd[0] * y, fwd[1] * y, h);
			line = line + " " + cell(c + (r[0] * 48, r[1] * 48, 0), c - (r[0] * 48, r[1] * 48, 0), target);
		}
		logPrint(line + "\n");
	}
}

log_spawn(who)
{
	if (isdefined(self.probe_spawnat))
		return;
	self.probe_spawnat = 1;
	spawns = getentarray("mp_teamdeathmatch_spawn", "classname");
	if (!spawns.size)
		spawns = getentarray("mp_tdm_spawn", "classname");
	best = undefined;
	for (i = 0; i < spawns.size; i++)
		if (!isdefined(best) || distance(spawns[i].origin, self.origin) < distance(best.origin, self.origin))
			best = spawns[i];
	if (isdefined(best))
	{
		o = best.origin;
		a = best.angles;
		logPrint("PROBE spawnat " + gettime() + " " + who + " " + o[0] + " " + o[1] + " " + o[2] + " " + a[0] + " " + a[1] + " " + a[2] + "\n");
	}
}

fall_log(who, spot)
{
	self notify("probe_fall");
	self endon("probe_fall");
	for (k = 0; k < 10; k++)
	{
		wait 0.05;
		logPrint("PROBE fall " + gettime() + " " + who + " " + (self.origin[2] - spot[2]) + "\n");
	}
}

//	Holds both players on their spots and the shooter's aim, every frame.
hold(site)
{
	logfall = getcvarint("probe_fall");
	for (;;)
	{
		shooter = team_player("allies");
		target = team_player("axis");
		if (isdefined(shooter) && isdefined(target))
		{
			if (distance(shooter.origin, site.a) > 0.5)
			{
				if (logfall)
					shooter log_spawn("shooter");
				shooter setorigin(site.a);
				if (logfall)
					shooter thread fall_log("shooter", site.a);
			}
			off = target.origin - site.b;
			if (level.probe_twb & 64)
				off = (off[0], off[1], 0);
			if (length(off) > 0.5)
			{
				if (logfall)
					target log_spawn("target");
				target setorigin(site.b);
				if (logfall)
					target thread fall_log("target", site.b);
			}
			shooter setWeaponSlotClipAmmo("primary", 5);
			shooter giveMaxAmmo(shooter getcurrentweapon());
			eye = site.a + (0, 0, 60) + lean_off(level.probe_swb, site.yaw);
			fwd = site.fwd;
			if (level.probe_twb & 64)
				at = site.b + (fwd[0] * 20, fwd[1] * 20, 8);
			else if (level.probe_twb & 128)
				at = site.b + (0, 0, 30);
			else
				at = site.b + (0, 0, 44);
			t = lean_off(level.probe_twb, site.yaw + 180);
			at = at + (t[0] * 0.5, t[1] * 0.5, t[2] * 0.5);
			shooter setplayerangles(vectortoangles(at - eye));
			target setplayerangles((0, site.yaw + 180, 0));
		}
		wait 0.05;
	}
}

run()
{
	level.probe_phase = "none";
	level.probe_twb = 0;
	level.probe_swb = 0;
	wait 1;
	site = pick_site();
	if (!isdefined(site))
	{
		logPrint("PROBE site none\n");
		return;
	}
	logPrint("PROBE site " + gettime() + " " + site.a + " " + site.b + " " + site.yaw + "\n");
	thread hold(site);
	for (;;)
	{
		if (isdefined(team_player("allies")) && isdefined(team_player("axis")))
			break;
		wait 0.05;
	}
	//	The target lies prone and stands, so his legs and torso face his
	//	view whatever yaw his spawn point gave them (cod11-combat.md 16.4);
	//	the shooter's rifle is raised and sighted.
	level.probe_twb = 64;
	send_in(team_player("axis"), 0, 64);
	send_in(team_player("allies"), 16, 0);
	wait 2.5;
	level.probe_twb = 0;
	send_in(team_player("axis"), 0, 0);
	wait 2.5;
	//	The phases start on a fixed level time, so the scoped sway and the
	//	idle clip read the same clock on any server however late the
	//	clients joined.
	while (gettime() < 20000)
		wait 0.05;
	if (getcvarint("probe_pin") && team_player("axis") getattachsize() == 4)
	{
		pin = [];
		pin[0] = "xmodel/basehead_axis_a_01";
		pin[1] = "xmodel/gear_german_helmet_falshrm_blk";
		pin[2] = "xmodel/gear_german_load1_falshrm";
		pin[3] = "xmodel/gear_german_bandolier";
		team_player("axis") reattach(pin);
	}
	team_player("axis") log_models();
	ps = phases();
	for (i = 0; i < ps.size; i++)
	{
		p = ps[i];
		shooter = team_player("allies");
		target = team_player("axis");
		if (!isdefined(shooter) || !isdefined(target))
			return;
		level.probe_phase = p.name;
		level.probe_twb = p.twb;
		level.probe_swb = p.swb;
		logPrint("PROBE phase " + gettime() + " " + p.name + "\n");
		send_in(target, 0, p.twb);
		send_in(shooter, 16, p.swb);
		if (getcvarint("probe_bones"))
			thread bones(p.name, site, target, p.settle - 0.35);
		wait p.settle;
		grid(p.name, site, target);
		if (p.name == "level" && getcvarint("probe_pin"))
		{
			target reattach(undefined);
			grid("level_re", site, target);
		}
		send_in(shooter, 17, p.swb);
		wait 0.05;
		send_in(shooter, 16, p.swb);
		wait 1.5;
	}
	logPrint("PROBE done " + gettime() + "\n");
}
