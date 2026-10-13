//	Map effects on the wire (docs/research/cod11-events-and-fx.md 2): what
//	playFx, playFx with a forward, playLoopedFx and playFxOnTag put in a
//	client's snapshots. The first allied player is put on mp_carentan's
//	street at (900 1930 -38); a second later the effects play at fixed
//	spots round it, each logged as `PROBE <step> <time> [<entity>]`:
//
//	plain   playFx at the player's feet
//	dir     playFx with forward (0 3 4)
//	far     playFx 8000 units off, outside the client's PVS
//	loop_a  playLoopedFx, 0.2 s, no cull, no forward
//	loop_b  playLoopedFx, 1.2345 s, cull 1000, forward (0 3 4)
//	loop_c  playLoopedFx, 0.5 s, cull 500, 8000 units off
//	tag     playFxOnTag on the player's tag_helmet
//	del_a   loop_a deleted
//	del_bc  loop_b and loop_c deleted
//
//	The client half is `vcod --net-probe --probe-team allies
//	--save-scripted fx`, which writes an `!fx` line per effect event and
//	per looped effect entity as it comes and goes.

main()
{
	level._effect["bomb"] = loadfx("fx/explosions/mp_bomb.efx");
	level._effect["smoke"] = loadfx("fx/smoke/chimneysmoke.efx");
	thread watch();
	maps\mp\gametypes\dm::main();
}

watch()
{
	for (;;)
	{
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
		{
			if (isdefined(players[i].sessionstate) && players[i].sessionstate == "playing")
			{
				players[i] effects();
				return;
			}
		}
		wait 0.05;
	}
}

effects()
{
	org = (900, 1930, -38);
	self setorigin(org);
	self setplayerangles((0, 90, 0));
	wait 1;

	step("plain", undefined);
	playfx(level._effect["bomb"], org + (0, 0, 8));
	wait 0.5;
	step("dir", undefined);
	playfx(level._effect["bomb"], org + (0, 64, 8), (0, 3, 4));
	wait 0.5;
	step("far", undefined);
	playfx(level._effect["bomb"], org + (8000, 8000, 0));
	wait 0.5;

	a = playLoopedFx(level._effect["smoke"], 0.2, org + (64, 0, 0));
	step("loop_a", a);
	b = playLoopedFx(level._effect["smoke"], 1.2345, org + (0, 128, 0), 1000, (0, 3, 4));
	step("loop_b", b);
	c = playLoopedFx(level._effect["smoke"], 0.5, org + (8000, 8000, 0), 500);
	step("loop_c", c);
	wait 1;

	step("tag", undefined);
	playfxontag(level._effect["bomb"], self, "tag_helmet");
	wait 1;

	step("del_a", undefined);
	a delete();
	wait 1;
	step("del_bc", undefined);
	b delete();
	c delete();
	wait 1;
	step("done", undefined);
}

step(name, ent)
{
	if (isdefined(ent))
		logPrint("PROBE " + name + " " + getTime() + " " + ent getEntityNumber() + "\n");
	else
		logPrint("PROBE " + name + " " + getTime() + "\n");
}
