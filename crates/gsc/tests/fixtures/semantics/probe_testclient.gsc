// addTestClient (game.mp.i386.so 0x5f6e8, cod_lnxded SV_AddTestClient
// 0x808753c): refused within sv_reconnectlimit of the start, then a
// client that connects and begins inside the call, named and numbered as
// retail picks (docs/research/cod11-gsc-object-model.md, "addTestClient").

main()
{
	maps\mp\gametypes\dm::main();
	level.callbackPlayerConnect = ::onconnect;
	thread run();
}

run()
{
	wait 0.05;
	ent = addtestclient();
	logPrint("PROBE early " + isDefined(ent) + "\n");
	wait 5;
	logPrint("PROBE before\n");
	ent = addtestclient();
	logPrint("PROBE returned " + isDefined(ent) + " player " + isPlayer(ent) + " num " + ent getEntityNumber() + " name " + ent.name + " state " + ent.sessionstate + " team " + ent.sessionteam + "\n");
	ent2 = addtestclient();
	logPrint("PROBE second num " + ent2 getEntityNumber() + " name " + ent2.name + "\n");
	wait 0.05;
	spot = getentarray("mp_deathmatch_spawn", "classname")[0];
	ent.sessionstate = "playing";
	ent spawn(spot.origin + (0, 0, 200), spot.angles);
	logPrint("PROBE spawned dz " + (ent.origin[2] - spot.origin[2]) + " health " + ent.health + "\n");
	wait 1;
	logPrint("PROBE after1s dz " + (ent.origin[2] - spot.origin[2]) + " ground " + ent isOnGround() + "\n");
	ent3 = addtestclient();
	logPrint("PROBE third num " + ent3 getEntityNumber() + " name " + ent3.name + "\n");
	logPrint("PROBE done\n");
}

onconnect()
{
	logPrint("PROBE connect " + self getEntityNumber() + " " + self.name + " state " + self.sessionstate + "\n");
	self waittill("begin");
	logPrint("PROBE begin " + self getEntityNumber() + "\n");
	wait 0.05;
	logPrint("PROBE connect_wait " + self getEntityNumber() + "\n");
}
