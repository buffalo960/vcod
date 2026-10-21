//	`isTouching` against a standing, crouched, prone and dead player
//	(docs/research/cod11-gsc-object-model.md 22.2, 22.3). Stock tdm on
//	mp_carentan under tools/run_probe.sh, one `--probe-uo-weapon direct`
//	axis client. The script puts him once on the flat brush floor under
//	the garden (probe_legs' spot), where he lands and stands still (a body
//	held there every frame floats a unit up, and 1.5's prone refuses an
//	airborne press), and drives his stance through `iPrintLn("PROBE_IN
//	...")`.
//	mp_carentan's trigger_hurt (`*1`, one axial brush, z -176 to -160 and
//	x from -8448) is the partner: each sweep moves it in one frame, asks
//	`isTouching` both ways at every step and puts it back before the
//	frame ends, so the touch pass never meets it.
//
//	`bottom` puts the brush's bottom face `v` over his feet, `top` its top
//	face `v` over them, `side` its -x face `v` along +x off his origin
//	(the brush then spans his feet 8 down to 8 up). `v` runs every 0.125
//	from `lo` to `hi`; each line is a step where the answer changed, the
//	first step included. `<pt>` is `player isTouching(trigger)` then
//	`trigger isTouching(player)`:
//
//	PROBE site <spot> <surface type under it>
//	PROBE phase <name> <sessionstate> <origin>
//	PROBE edge <phase> <bottom|top|side> <v> <pt>
//
//	Between `dead_now` and `dead_crouched` the corpse's origin, one line a
//	frame for eight frames:
//
//	PROBE fall <frame> <x> <y> <z>

main()
{
	thread run();
	maps\mp\gametypes\tdm::main();
	level.callbackPlayerDamage = ::no_damage;
}

no_damage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc)
{
}

in(wbuttons)
{
	level.probe_player iprintln("PROBE_IN 0 " + wbuttons + " 0");
}

answer(p, t)
{
	a = "0";
	if (p istouching(t))
		a = "1";
	if (t istouching(p))
		return a + "1";
	return a + "0";
}

sweep(p, t, phase, axis, lo, hi)
{
	o = p.origin;
	last = "";
	for (i = lo * 8; i <= hi * 8; i++)
	{
		v = i * 0.125;
		if (axis == "bottom")
			t.origin = (o[0], o[1], o[2] + 176 + v);
		else if (axis == "top")
			t.origin = (o[0], o[1], o[2] + 160 + v);
		else
			t.origin = (o[0] + 8448 + v, o[1], o[2] + 168);
		a = answer(p, t);
		if (a != last)
			logPrint("PROBE edge " + phase + " " + axis + " " + v + " " + a + "\n");
		last = a;
	}
	t.origin = (0, 0, 0);
}

measure(p, t, name)
{
	logPrint("PROBE phase " + name + " " + p.sessionstate + " " + p.origin + "\n");
	sweep(p, t, name, "bottom", 0, 80);
	sweep(p, t, name, "top", -4, 4);
	sweep(p, t, name, "side", -4, 24);
}

run()
{
	wait 1;
	t = getent("trigger_hurt", "classname");
	level.spot = (932, -376, -151.875);
	logPrint("PROBE site " + level.spot + " " + bullettrace(level.spot + (0, 0, 8), level.spot - (0, 0, 8), false, undefined)["surfacetype"] + "\n");
	for (;;)
	{
		pl = getentarray("player", "classname");
		if (pl.size > 0 && pl[0].sessionstate == "playing")
			break;
		wait 0.05;
	}
	p = pl[0];
	level.probe_player = p;
	p setorigin(level.spot);
	in(0);
	wait 2;
	measure(p, t, "stand");
	in(128);
	wait 1.5;
	measure(p, t, "crouch");
	in(0);
	wait 2;
	measure(p, t, "stand_again");
	in(64);
	wait 2;
	measure(p, t, "prone");
	in(128);
	wait 2;
	measure(p, t, "crouch_again");
	p suicide();
	measure(p, t, "dead_now");
	for (i = 1; i <= 8; i++)
	{
		wait 0.05;
		o = p.origin;
		logPrint("PROBE fall " + i + " " + o[0] + " " + o[1] + " " + o[2] + "\n");
	}
	measure(p, t, "dead_crouched");
	in(0);
	wait 1.5;
	measure(p, t, "dead");
	logPrint("PROBE done\n");
}
