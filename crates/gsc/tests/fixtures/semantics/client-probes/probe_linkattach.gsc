//	linkTo on a tag that lives on an attached model, mp_carentan, no client
//	needed. A script_model wearing a body gets the thompson attached on
//	"tag_weapon_right"; k1 links to the thompson's "tag_flash" with zero
//	offsets, k2 to the same tag keeping its pose. Logs every server frame of
//	a phase:
//
//		PROBE m <phase> <time> <m origin> <m angles> <k1 origin> <k1 angles> <k2 origin> <k2 angles>
//
//	Phases: the parent moves and turns (att_move); the thompson detached
//	(att_detach); attached again, k1 relinked and a second model attached
//	after it (att_second); detachAll (att_detachall); a link on the
//	attachment's own root bone "tag_weapon" (att_root). Ends on a link to
//	"tag_flash" with nothing attached, whose fatal is the last measurement.
//	Run by tools/run_probe.sh with no client.

main()
{
	precacheModel("xmodel/playerbody_german_wehrmacht");
	precacheModel("xmodel/weapon_thompson");
	precacheModel("xmodel/weapon_mp40");
	thread phases();
	maps\mp\gametypes\dm::main();
}

phases()
{
	wait 2;
	if (getcvar("mapname") != "mp_carentan")
	{
		logPrint("PROBE unsupported " + getcvar("mapname") + "\n");
		return;
	}
	m = spawn("script_model", (-512, 2688, 64));
	m.angles = (0, 30, 0);
	m setmodel("xmodel/playerbody_german_wehrmacht");
	m attach("xmodel/weapon_thompson", "tag_weapon_right");
	k1 = spawn("script_origin", m.origin);
	k2 = spawn("script_origin", m.origin + (10, 0, 50));
	k2.angles = (0, 90, 0);
	logPrint("PROBE ents m " + m getEntityNumber() + " k1 " + k1 getEntityNumber() + " k2 " + k2 getEntityNumber() + "\n");
	logPrint("PROBE at link_flash " + getTime() + "\n");
	k1 linkto(m, "tag_flash", (0, 0, 0), (0, 0, 0));
	k2 linkto(m, "tag_flash");
	sample("linked", m, k1, k2, 0.3);
	m movez(16, 1);
	m rotateyaw(45, 1);
	sample("att_move", m, k1, k2, 1.2);

	m detach("xmodel/weapon_thompson", "tag_weapon_right");
	m movez(-16, 1);
	sample("att_detach", m, k1, k2, 1.2);

	m attach("xmodel/weapon_thompson", "tag_weapon_right");
	k1 linkto(m, "tag_flash", (0, 0, 0), (0, 0, 0));
	k2 linkto(m, "tag_flash");
	sample("att_relinked", m, k1, k2, 0.3);
	m attach("xmodel/weapon_mp40", "tag_weapon_left");
	m movez(16, 1);
	sample("att_second", m, k1, k2, 1.2);

	m detachall();
	m movez(-16, 1);
	sample("att_detachall", m, k1, k2, 1.2);

	m attach("xmodel/weapon_thompson", "tag_weapon_right");
	logPrint("PROBE at link_root " + getTime() + "\n");
	k1 linkto(m, "tag_weapon", (0, 0, 0), (0, 0, 0));
	k2 linkto(m, "tag_weapon_right", (0, 0, 0), (0, 0, 0));
	sample("att_root", m, k1, k2, 0.3);

	m detachall();
	logPrint("PROBE at link_gone " + getTime() + "\n");
	k1 linkto(m, "tag_flash", (0, 0, 0), (0, 0, 0));
	logPrint("PROBE link_gone_survived\n");
}

sample(phase, m, k1, k2, secs)
{
	frames = secs * 20;
	for (i = 0; i < frames; i++)
	{
		logPrint("PROBE m " + phase + " " + getTime() + " " + m.origin + " " + m.angles + " " + k1.origin + " " + k1.angles + " " + k2.origin + " " + k2.angles + "\n");
		wait 0.05;
	}
}
