//	Mounted MG's server half: places a gunner behind mp_carentan's turret at
//	(1712 1830 8) and a target in front of it, logs every turret once. The
//	hits themselves are the engine's D;/K; lines. Run by tools/run_probe.sh
//	with a --save-turret client (allies) and a --probe-team axis client.

main()
{
	thread log_turrets();
	thread watch_teleports();
	thread watch_delete();
	thread watch_pose();
	thread watch_respawn();
	maps\mp\gametypes\dm::main();
}

//	With `probe_delete_after N`: the capture's gun is deleted N seconds in.
//	Only the A/B rig sets it (the release-on-delete test); retail runs leave
//	it unset.
watch_delete()
{
	secs = getcvarint("probe_delete_after");
	if (secs <= 0)
		return;
	wait secs;
	gun = probe_gun();
	if (isdefined(gun))
		gun delete();
}

log_turrets()
{
	wait 1;
	guns = getentarray("misc_mg42", "classname");
	for (i = 0; i < guns.size; i++)
		logPrint("PROBE turret " + guns[i] getEntityNumber() + " " + guns[i].origin + " " + guns[i].angles + "\n");
}

//	The gun the capture uses: the one nearest (1712 1830 8).
probe_gun()
{
	guns = getentarray("misc_mg42", "classname");
	best = undefined;
	for (i = 0; i < guns.size; i++)
	{
		if (!isdefined(best) || distance(guns[i].origin, (1712, 1830, 8)) < distance(best.origin, (1712, 1830, 8)))
			best = guns[i];
	}
	return best;
}

//	With `probe_teleport 1`: every allied player is put 40 units behind the
//	gun facing along it, every axis player 300 units in front facing back,
//	once per spawn, the first frame its sessionstate reads "playing":
//
//	PROBE place <entity number> <team> <spot> <yaw> <time>
watch_teleports()
{
	if (getcvar("probe_teleport") != "1")
		return;
	wait 1;
	gun = probe_gun();
	if (!isdefined(gun))
	{
		logPrint("PROBE teleport unsupported " + getcvar("mapname") + "\n");
		return;
	}
	for (;;)
	{
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
			players[i] try_place(gun);
		wait 0.05;
	}
}

//	Early returns rather than one compound test, the way probe_pickup's
//	try_teleport guards sessionstate: it is undefined before the first spawn.
try_place(gun)
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
	fwd = anglestoforward(gun.angles);
	if (self.pers["team"] == "allies")
	{
		spot = gun.origin - (fwd[0] * 40, fwd[1] * 40, 0);
		yaw = gun.angles[1];
	}
	else
	{
		spot = gun.origin + (fwd[0] * 300, fwd[1] * 300, 0);
		yaw = gun.angles[1] + 180;
	}
	self.probe_spot = spot;
	self setorigin(spot);
	self setplayerangles((0, yaw, 0));
	logPrint("PROBE place " + self getEntityNumber() + " " + self.pers["team"] + " " + spot + " " + yaw + " " + getTime() + "\n");
}

//	With `probe_pose 1`: which pose a turret round meets on a body whose own
//	end frame has not run yet (combat doc 16.1). Every hit logs its time,
//	location and point instead of landing; the axis client flips its own view
//	pitch (`--probe-pitch-flip`) and logs the snapshot each flip reached.
watch_pose()
{
	if (getcvar("probe_pose") != "1")
		return;
	wait 1;
	level.callbackPlayerDamage = ::pose_damage;
}

pose_damage(eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc)
{
	logPrint("PROBE hit " + gettime() + " " + self getEntityNumber() + " " + sHitLoc + " " + iDamage + " " + vPoint + "\n");
}

//	With `probe_respawn 1`: a second after the allied gunner is on the gun
//	(his origin left the placement spot across the ground), the script spawns him again as a
//	player 40 units further back, facing 90 off the gun, and logs what it
//	reads straight after the `spawn` and a frame later, once per spawn:
//
//	PROBE seat <time> <entity number> <origin> <angles>
//	PROBE seatnext <time> <entity number> <origin> <angles>
watch_respawn()
{
	if (getcvar("probe_respawn") != "1")
		return;
	wait 1;
	gun = probe_gun();
	for (;;)
	{
		players = getentarray("player", "classname");
		for (i = 0; i < players.size; i++)
			players[i] try_respawn(gun);
		wait 0.05;
	}
}

try_respawn(gun)
{
	if (!isdefined(self.probe_spot))
		return;
	if (isdefined(self.probe_respawned))
		return;
	if (self.pers["team"] != "allies")
		return;
	d = self.origin - self.probe_spot;
	if (d[0] * d[0] + d[1] * d[1] < 16)
		return;
	self.probe_respawned = 1;
	self thread respawn(gun);
}

respawn(gun)
{
	self endon("disconnect");
	wait 1;
	fwd = anglestoforward(gun.angles);
	spot = self.probe_spot - (fwd[0] * 40, fwd[1] * 40, 0);
	self spawn(spot, (0, gun.angles[1] + 90, 0));
	logPrint("PROBE seat " + getTime() + " " + self getEntityNumber() + " " + self.origin + " " + self.angles + "\n");
	wait 0.05;
	logPrint("PROBE seatnext " + getTime() + " " + self getEntityNumber() + " " + self.origin + " " + self.angles + "\n");
}
