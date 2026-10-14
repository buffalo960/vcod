//	A blast on a United Offensive script model that setTakeDamage opened: the
//	six arguments of its "damage" notify. Needs no client; mp_cassino under
//	COD_VERSION=uo tools/run_probe.sh. The tank scripts die on this gametype
//	a few seconds in (no level.ceasefire), after the two lines.
//	crates/server/tests/blast_entity_uo_ab.rs holds retail's lines;
//	docs/research/coduo-diff.md 22.

main()
{
	level.callbackStartGameType = ::cb;
	level.callbackPlayerConnect = ::cb;
	level.callbackPlayerDisconnect = ::cb;
	level.callbackPlayerDamage = ::cb;
	level.callbackPlayerKilled = ::cb;
	maps\mp\gametypes\_callbacksetup::SetupCallbacks();
	thread go();
}

cb() {}

go()
{
	m = spawn("script_model", (100, 200, 0));
	m setmodel("xmodel/health_medium");
	m settakedamage(true);
	m thread w();
	wait 1;
	radiusDamage((100, 200, 10), 200, 100, 50);
	wait 1;
	logPrint("PROBE end\n");
}

w()
{
	self waittill("damage", dmg, who, dir, point, mod, inflictor);
	logPrint("PROBE dmg " + dmg + " " + mod + " " + dir + " " + point + "\n");
}
