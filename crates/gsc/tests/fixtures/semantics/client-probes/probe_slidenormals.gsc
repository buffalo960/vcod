//	bulletTrace normals of mp_streets' terrain triangle 1536 and patch 462
//	by the UO crouchjump-low capture's ct 22491 (cod15-movement.md 7):
//	each component's residual against the float vcod's collision builds,
//	so a one-ulp difference prints. Run by tools/run_probe.sh on
//	mp_streets with COD_VERSION=uo; no client needed.

main()
{
	maps\mp\gametypes\dm::main();
	thread probe();
}

probe()
{
	wait 1;
	ground = (0.13010798, 0.019151546, 0.9913149);
	patch = (-0.66601574, 0.74593765, 0);
	shot("down0", (527.2, -3550.86, -140), (527.2, -3550.86, -200), ground);
	shot("down1", (527, -3550, -140), (527, -3550, -200), ground);
	shot("down2", (530, -3548, -140), (530, -3548, -200), ground);
	shot("down3", (525, -3549, -140), (525, -3549, -200), ground);
	d = (26.6406296, -29.837506, 0);
	shot("patch0", (527.2, -3550.86, -150), (527.2, -3550.86, -150) + d, patch);
	shot("patch1", (527.2, -3550.86, -140), (527.2, -3550.86, -140) + d, patch);
	shot("patch2", (527.2, -3550.86, -130), (527.2, -3550.86, -130) + d, patch);
	logPrint("PROBE ref " + ground[0] + " " + ground[1] + " " + ground[2] + " " + patch[0] + " " + patch[1] + "\n");
	logPrint("PROBE done\n");
}

shot(name, from, to, ref)
{
	t = bulletTrace(from, to, false, undefined);
	n = t["normal"];
	logPrint("PROBE " + name + " " + t["fraction"] + " " + t["position"] + " " + t["surfacetype"] + " n " + n + " r " + (n[0] - ref[0]) + " " + (n[1] - ref[1]) + " " + (n[2] - ref[2]) + "\n");
}
