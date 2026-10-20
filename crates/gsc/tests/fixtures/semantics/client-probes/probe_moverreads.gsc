//	Mover script reads no other probe measured, mp_carentan, no client, for
//	docs/research/cod11-movers.md 23:
//	- s: .speed and .dmg on a script-spawned func_bobbing and func_pendulum
//	  (both deleted in the frame they spawn, before their period drops the
//	  level, section 18) and on a script_model nothing wrote them on;
//	- g: moveGravity on a mover in the middle of a moveZ, of another
//	  moveGravity and at rest: .origin right before and right after the
//	  call, then every frame;
//	- r: .angles every frame through rotates that cross below 0 or past
//	  360, a rotateVelocity and a rotateTo, angles times 1e6.
//
//		PROBE s <kind> <speed> <dmg>
//		PROBE g <phase> <step> <time> <x> <y> <z>
//		PROBE r <phase> <time> <pitch * 1e6> <yaw * 1e6> <roll * 1e6>
//
//	Run: tools/run_probe.sh client-probes/probe_moverreads mp_carentan

main()
{
	thread run();
	maps\mp\gametypes\dm::main();
}

run()
{
	wait 1;
	b = spawn("func_bobbing", (0, 0, 0));
	p = spawn("func_pendulum", (0, 0, 0));
	m = spawn("script_model", (-400, 2600, 40));
	logPrint("PROBE s bobbing " + b.speed + " " + b.dmg + "\n");
	logPrint("PROBE s pendulum " + p.speed + " " + p.dmg + "\n");
	logPrint("PROBE s model " + m.speed + " " + m.dmg + "\n");
	b delete();
	p delete();
	m delete();

	o = spawn("script_origin", (-400, 2600, 40));
	wait 0.05;
	o movez(100, 1);
	gsample("z", o, 0.5);
	gshow("z", "before", o);
	o movegravity((20, 0, 50), 1);
	gshow("z", "after", o);
	gsample("grav", o, 0.35);
	gshow("grav", "before", o);
	o movegravity((0, -10, 0), 1);
	gshow("grav", "after", o);
	gsample("grav2", o, 1.2);
	gshow("rest", "before", o);
	o movegravity((0, 0, 100), 0.5);
	gshow("rest", "after", o);
	gsample("rest", o, 0.6);
	o delete();

	r = spawn("script_origin", (-400, 2600, 40));
	wait 0.05;
	r rotateyaw(-90, 1);
	rsample("yaw", r, 1.2);
	r rotatepitch(-30, 0.5);
	rsample("pitch", r, 0.7);
	r rotateroll(-400, 1);
	rsample("roll", r, 1.2);
	r rotateyaw(500, 1);
	rsample("yaw500", r, 1.2);
	r rotatevelocity((-40, -120, 0), 1);
	rsample("vel", r, 1.2);
	r rotateto((10, -170, -20), 1);
	rsample("to", r, 1.2);
	logPrint("PROBE done\n");
}

gshow(phase, step, e)
{
	o = e.origin;
	logPrint("PROBE g " + phase + " " + step + " " + getTime() + " " + o[0] + " " + o[1] + " " + o[2] + "\n");
}

gsample(phase, e, secs)
{
	for (t = 0; t < secs; t += 0.05)
	{
		gshow(phase, "f", e);
		wait 0.05;
	}
}

rsample(phase, e, secs)
{
	for (t = 0; t < secs; t += 0.05)
	{
		a = e.angles;
		logPrint("PROBE r " + phase + " " + getTime() + " " + a[0] * 1000000 + " " + a[1] * 1000000 + " " + a[2] * 1000000 + "\n");
		wait 0.05;
	}
}
