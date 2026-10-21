//	`useBy` on the entities whose `use` is not hurt_use, for
//	docs/research/cod11-combat.md 5.9. mp_carentan only, no client. Each
//	phase spawns its entity at the world origin, so a trigger's inline model
//	0 lies exactly on the world and nothing touches it, waits half a second,
//	then calls `useBy` with a script_origin:
//
//	- once: a trigger_once (wait -1): Use_Multi spends it.
//	- multiple: a trigger_multiple (wait 0.5): Use_Multi arms multi_wait.
//	- damage: a trigger_damage: Use_trigger_damage's activation.
//	- static: a func_static lifted 8 by an origin write: Use_Static
//	  unlinks it and the next use links it again, which a bulletTrace down
//	  on to the street at (900 1930) shows (the copy's street is 8 higher).
//
//	+set probe_life 1, with mp_carentan's trigger_hurt given "life" "2.5"
//	in place of its "dmg" "1000" (a same-length entity lump edit, the
//	fixture header has it), runs only the life phase: the slab's useBy arms
//	hurt_think, which frees it 2.5 s on.
//
//	+set probe_sine 1 runs only the sine phases: a script-spawned
//	func_bobbing and func_pendulum, each logged a frame at a time from half
//	a second before its first useBy (Use_BinaryMover) to a second past its
//	second.
//
//	+set probe_spawner 1 runs only the spawner phases: a script-spawned
//	misc_spawner (no spawnitem, so SP_misc_spawner installs no use), then
//	the map's, which needs mp_carentan's first mp_retrieval_spawn_axis
//	turned into a misc_spawner with "spawnitem" "item_health" (a
//	same-length entity lump edit, the fixture header has it). Each is used
//	twice a second apart and every item_health is logged each frame.
//
//	+set probe_turret 1 waits for a playing player, puts him 40 units behind
//	mp_carentan's gun at (1712 1830 8) looking 60 degrees off its yaw and 10
//	down, and a second on calls the gun's useBy with him (turret_use): the
//	player is logged each frame from 0.25 s before to 1.5 s after. Run it
//	with one --probe-ride client. +set probe_turret 2 uses the gun with the
//	script_origin instead.
//
//	PROBE user <num>
//	PROBE spawned <phase> <num> <classname>
//	PROBE use <phase> <time>
//	PROBE trigger <phase> <time> <num>        every "trigger" notify
//	PROBE def <phase> <time> <isdefined>
//	PROBE trace <phase> <time> <z> <entity num or world>
//	PROBE sine <phase> <time> <origin> <angles>
//	PROBE item <phase> <time> <count> [<num> <origin>]...
//	PROBE tplace <num> <time> <origin> <angles>
//	PROBE t <time> <origin> <player angles field> <weapon>
//	PROBE done
//	Run by tools/run_probe.sh client-probes/probe_useby mp_carentan.

main()
{
	thread run();
	maps\mp\gametypes\dm::main();
}

run()
{
	if (getcvar("mapname") != "mp_carentan")
	{
		logPrint("PROBE unsupported " + getcvar("mapname") + "\n");
		return;
	}
	wait 1;
	user = spawn("script_origin", (0, 0, 0));
	logPrint("PROBE user " + user getEntityNumber() + "\n");
	if (getcvar("probe_life") != "")
	{
		life(user);
		logPrint("PROBE done\n");
		return;
	}
	if (getcvar("probe_sine") != "")
	{
		use_sine(user, "bobbing", "func_bobbing");
		use_sine(user, "pendulum", "func_pendulum");
		logPrint("PROBE done\n");
		return;
	}
	if (getcvar("probe_spawner") != "")
	{
		m = getent("misc_spawner", "classname");
		s = spawn("misc_spawner", (0, 0, 0));
		logPrint("PROBE spawned scriptspawner " + s getEntityNumber() + " " + s.classname + "\n");
		s.spawnitem = "item_health";
		use_spawner(user, s, "scriptspawner");
		logPrint("PROBE spawned mapspawner " + m getEntityNumber() + " " + m.classname + " " + m.origin + "\n");
		use_spawner(user, m, "mapspawner");
		logPrint("PROBE done\n");
		return;
	}
	if (getcvar("probe_turret") != "")
	{
		use_turret(user);
		logPrint("PROBE done\n");
		return;
	}
	use_trigger(user, "once", "trigger_once");
	use_trigger(user, "multiple", "trigger_multiple");
	use_trigger(user, "damage", "trigger_damage");
	use_static(user);
	logPrint("PROBE done\n");
}

watch(phase)
{
	for (;;)
	{
		self waittill("trigger", who);
		logPrint("PROBE trigger " + phase + " " + getTime() + " " + who getEntityNumber() + "\n");
	}
}

defined(e, phase, frames)
{
	for (i = 0; i < frames; i++)
	{
		logPrint("PROBE def " + phase + " " + getTime() + " " + isdefined(e) + "\n");
		wait 0.05;
	}
}

use_trigger(user, phase, classname)
{
	t = spawn(classname, (0, 0, 0));
	logPrint("PROBE spawned " + phase + " " + t getEntityNumber() + " " + t.classname + "\n");
	t thread watch(phase);
	wait 0.5;
	logPrint("PROBE use " + phase + " " + getTime() + "\n");
	t useby(user);
	defined(t, phase, 5);
	if (isdefined(t))
	{
		logPrint("PROBE use " + phase + " " + getTime() + "\n");
		t useby(user);
		defined(t, phase, 3);
	}
}

trace(phase)
{
	tr = bulletTrace((900, 1930, 100), (900, 1930, -200), false, undefined);
	hit = "world";
	if (isdefined(tr["entity"]))
		hit = "ent" + tr["entity"] getEntityNumber();
	logPrint("PROBE trace " + phase + " " + getTime() + " " + tr["position"][2] + " " + hit + "\n");
}

use_static(user)
{
	s = spawn("func_static", (0, 0, 0));
	logPrint("PROBE spawned static " + s getEntityNumber() + " " + s.classname + "\n");
	s.origin = (0, 0, 8);
	s thread watch("static");
	wait 0.5;
	trace("static");
	logPrint("PROBE use static " + getTime() + "\n");
	s useby(user);
	trace("static");
	wait 0.05;
	trace("static");
	logPrint("PROBE use static " + getTime() + "\n");
	s useby(user);
	trace("static");
	wait 0.05;
	trace("static");
}

life(user)
{
	hurt = getent("trigger_hurt", "classname");
	logPrint("PROBE spawned life " + hurt getEntityNumber() + " " + hurt.classname + "\n");
	hurt thread watch("life");
	wait 0.5;
	logPrint("PROBE use life " + getTime() + "\n");
	hurt useby(user);
	defined(hurt, "life", 60);
}

sine_frames(e, phase, frames)
{
	for (i = 0; i < frames; i++)
	{
		logPrint("PROBE sine " + phase + " " + getTime() + " " + e.origin + " " + e.angles + "\n");
		wait 0.05;
	}
}

use_sine(user, phase, classname)
{
	m = spawn(classname, (0, 0, 0));
	logPrint("PROBE spawned " + phase + " " + m getEntityNumber() + " " + m.classname + "\n");
	m thread watch(phase);
	sine_frames(m, phase, 10);
	logPrint("PROBE use " + phase + " " + getTime() + "\n");
	m useby(user);
	sine_frames(m, phase, 20);
	logPrint("PROBE use " + phase + " " + getTime() + "\n");
	m useby(user);
	sine_frames(m, phase, 20);
}

item_frames(phase, frames)
{
	for (i = 0; i < frames; i++)
	{
		items = getentarray("item_health", "classname");
		line = "PROBE item " + phase + " " + getTime() + " " + items.size;
		for (j = 0; j < items.size; j++)
			line = line + " " + items[j] getEntityNumber() + " " + items[j].origin;
		logPrint(line + "\n");
		wait 0.05;
	}
}

use_spawner(user, s, phase)
{
	s thread watch(phase);
	wait 0.5;
	logPrint("PROBE use " + phase + " " + getTime() + "\n");
	s useby(user);
	item_frames(phase, 20);
	logPrint("PROBE use " + phase + " " + getTime() + "\n");
	s useby(user);
	item_frames(phase, 20);
}

tframe(p)
{
	logPrint("PROBE t " + getTime() + " " + p.origin + " " + p.angles + " " + p getCurrentWeapon() + "\n");
}

use_turret(user)
{
	guns = getentarray("misc_mg42", "classname");
	gun = undefined;
	for (i = 0; i < guns.size; i++)
		if (distance(guns[i].origin, (1712, 1830, 8)) < 1)
			gun = guns[i];
	logPrint("PROBE spawned turret " + gun getEntityNumber() + " " + gun.classname + " " + gun.origin + " " + gun.angles + "\n");
	gun thread watch("turret");
	if (getcvar("probe_turret") == "2")
	{
		logPrint("PROBE use turret " + getTime() + "\n");
		gun useby(user);
		wait 0.05;
		return;
	}
	p = undefined;
	while (!isdefined(p))
	{
		players = getentarray("player", "classname");
		for (j = 0; j < players.size; j++)
			if (isdefined(players[j].sessionstate) && players[j].sessionstate == "playing")
				p = players[j];
		wait 0.05;
	}
	fwd = anglestoforward(gun.angles);
	spot = gun.origin - (fwd[0] * 40, fwd[1] * 40, 0);
	p setorigin(spot);
	p setplayerangles((10, gun.angles[1] + 60, 0));
	logPrint("PROBE tplace " + p getEntityNumber() + " " + getTime() + " " + spot + " " + p.angles + "\n");
	wait 0.75;
	for (i = 0; i < 5; i++)
	{
		tframe(p);
		wait 0.05;
	}
	logPrint("PROBE use turret " + getTime() + "\n");
	gun useby(p);
	tframe(p);
	for (i = 0; i < 30; i++)
	{
		wait 0.05;
		tframe(p);
	}
}
