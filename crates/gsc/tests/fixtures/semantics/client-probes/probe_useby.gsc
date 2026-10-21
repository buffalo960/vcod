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
//	PROBE user <num>
//	PROBE spawned <phase> <num> <classname>
//	PROBE use <phase> <time>
//	PROBE trigger <phase> <time> <num>        every "trigger" notify
//	PROBE def <phase> <time> <isdefined>
//	PROBE trace <phase> <time> <z> <entity num or world>
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
