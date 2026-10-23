# CoD 1 xanim format (version 14)

The base byte layout of a CoD 1 animation clip, verified against the ten `xanim/viewmodel_kar98mp_*` clips and then against every `xanim/*` entry in the shipped pk3s: all 2957 decode to exactly EOF. The parser is `crates/common/src/xanim.rs`; `crates/common/src/skeleton.rs` applies the sampled keys to a pose. The format has no length prefixes, so landing exactly on the last notetrack byte is the oracle every layout rule below was checked with. This document covers the flag-0 layout; the three flagged variants (loop, delta, both) and the frame-index widening are decoded in `player-model-anim-system.md` under "xanim v14 header flags" and are only summarized here. Everything is VERIFIED unless labelled otherwise. Multi-byte values are little-endian; `cstr` is a NUL-terminated string.

## Layout (flags == 0)

```
u16 version                 // 14
u16 frame_count             // >= 1; 161 is the largest viewmodel clip, 256 the largest u8-indexed clip
u16 bone_count
u8  flags                   // 0 plain, 0x1 loop, 0x2 delta (root motion), 0x3 both
u16 framerate               // 30 for every viewmodel clip
u8[ceil(bone_count / 8)]    // bitset A: bit i set = negate bone i's first rotation key
u8[ceil(bone_count / 8)]    // bitset B: bit i set = bone i uses "simple" one-component rotation keys
bone_count x cstr           // track names, e.g. "tag_torso", "kar98_bolt"; bit index order matches
per bone, in name order:
  u16 rot_count             // <= frame_count
  if 1 < rot_count < frame_count: u8 frame_index[rot_count]   // ascending
    // rot_count == frame_count: implicit indices 0..frame_count-1
    // rot_count <= 1: constant or empty track, no index list
  rot_count x key:
    simple bone: i16 z                  // q = (0, 0, z / 32768, w)
    full bone:   i16 x, i16 y, i16 z    // q = (x, y, z) / 32768, w derived
  u16 trans_count           // <= frame_count, same index-list rule
  trans_count x f32[3]      // local bone position offset, see below
u8 note_count               // notetracks ("fire", "reload done", ...)
note_count x { cstr name; u16 frame }
```

- Dequantization matches `xmodelparts`: `q_xyz = v / 32768.0`, `q_w = sqrt(max(0, 1 - x^2 - y^2 - z^2))`. The clamp is required (largest observed `|xyz|^2` is 1.00003).
- Simple bones key a single i16 that is the quaternion's Z component. `skeleton.rs` pins the axis: read as Z, the `viewmodel_kar98mp_idle` finger keys equal the hands' `xmodelparts` bind locals exactly (0.0 degrees apart), while an X reading lands 6 to 114 degrees off. In the 48-bone kar98mp clips the simple bones are the forearms, every finger bone and `kar98_bolt`, all single-axis rotators.
- The parser refuses any flag value above 3; none occurs in the shipped corpus.

## Flag extensions, in brief

Decoded in `player-model-anim-system.md`. Flag `0x1` (loop) stores `frame_count + 1` key positions, the extra one being the loop-closing repeat of frame 0; vcod folds it into `XAnim::frame_count` so key indices stay in `0..frame_count` and `duration()` is the loop period. Flag `0x2` (delta, root motion) inserts one nameless track between the header and the bitsets, shaped like a bone track with yaw-only i16 rotations; vcod keeps it as `XAnim::root`, which only the mounted gunner's body placement reads (`cod11-turrets.md` section 7). Flag `0x3` composes both. Independently of the flags, the sparse frame-index list is u8 while the highest index fits a byte and u16 above that (`frame_count > 256` in the parser, counting the loop key).

## Timing

Duration is `(frame_count - 1) / framerate`, which reproduces the weapon-file timings: `viewmodel_kar98mp_fire` has 11 frames, 0.333 s, matching `fireTime 0.33`; `ADS_up` has 10 frames, 0.3 s, matching `adsTransInTime`. Looping playback wraps over `frame_count - 1`; one-shots clamp to the last frame and hold their end pose.

kar98mp clips (frames / tracks): idle 1/48, fire 11/48, lastshot 11/48, rechamber 32/48, reload 78/48, ADS_up 10/1, ADS_down 17/1, pullout 10/48, putaway 11/48. The two ADS clips animate only `tag_torso`, which moves the grafted gun along with the hands (see `xmodel-v14-format.md`).

## The sight layer

The hip position of a viewmodel is not its bind pose. Every `adsUpAnim` clip opens and every `adsDownAnim` closes on the hip placement of `tag_torso`, and `adsUpAnim` closes on the sight placement: for `kar98k_mp` `tag_torso` translates by (5.98, -3.10, -3.39) at `viewmodel_kar98_ADS_up` frame 0 and `viewmodel_kar98_ADS_down` frame 12, and by (3.66, -0.26, -0.51) at `ADS_up` frame 10 (VERIFIED, both clips). No idle, fire, reload or raise clip of a rifle, pistol or submachine gun keys `tag_torso` at all; the stock grenades key it in every clip and ship no ADS clips, and `panzerfaust_mp` keys it in `altDropAnim` only (VERIFIED, every `weapons/mp/*` file's named clips). So a rig that draws only the main clip leaves `tag_torso` at its zeroed bind, which holds the gun 6 units behind the hip and 3 up and left, the receiver in the camera's face.

In `cgame_mp_x86.dll` (1.1) the viewmodel anim tree is created with 21 nodes (`0x30034cf0`, trap `0x84` on the `"VIEWMODEL"` tree). The function at `0x30034240` sets a goal weight (trap `0x8c`) of 1 on node `0x13` and 0 on node `0x14`, or the reverse, and then sets node `0x13`'s time (trap `0x91`) to the value at `0x30207214` and node `0x14`'s to 1 minus it (VERIFIED, the immediates). `0x30207214` is the interpolated playerstate float at `+0xc4`, the one the fov function at `0x30032e20` blends `adsZoomFov` with, so `fWeaponPosFrac` (INFERRED). Its one caller at `0x3003448a` passes `0x13` when `pm_flags` bit `0x20` is set and `0x14` otherwise, with a weaponstate-5 case that forces `0x14` while `weaponTime` exceeds a weapon-def field (INFERRED, branch conditions). Read together, nodes `0x13` and `0x14` are the up and down sight clips, one of them always at full weight with its time slaved to the fraction (INFERRED), which puts the hip at `adsDownAnim`'s last frame. `ViewWeapon::pose` in `crates/client/src/viewmodel.rs` blends that layer with the main clip each frame ("United Offensive's rigs" below); it picks the up clip off the fraction's trend rather than the flag.

### The hip pose against retail

The "receiver in the camera's face" pose reported on 2026-09-25 for the kar98k in `--walk` and online was the missing sight layer above, fixed in `afebafc`; no per-weapon offset is involved. Every stock weapon file at rest (sight layer down, idle frame 0) holds `tag_weapon` 2.5 to 13.8 units ahead of `tag_view`, 2.4 to 7.8 right and 2.8 to 11.0 below (the kar98k at (4.33, -2.88, -3.84), the rifles at the short end, pistols, grenades and the panzerfaust at the long one); with the main clip alone the BAR's sits 1.7 behind the eye (VERIFIED, every `weapons/mp/*` rig through `crates/client/src/viewmodel.rs`; `every_stock_weapon_rests_ahead_right_and_below` pins it). The stand offsets play no part at rest: the cgame's `cg_gunX/Y/Z` cvars (table entries `0x30074cb4`, `0x30074cc4`, `0x30074cd4`, all default `"0"`) and the weapon files' `standMove*`/`standRot*` fields only add speed-scaled motion (VERIFIED, cvar table and the field table at `0x30075828`; that the stand terms are speed-scaled is INFERRED from `0x300362c0`'s branch on `standMoveMinSpeed`; `cod11-combat.md` 17.4 reads the function out, `duckedOfs`/`proneOfs` included).

Measured on screen, VERIFIED: a 1024x768 retail `CoDMP.exe` 1.1 under Wine and vcod `--walk` at the same size put the kar98k's muzzle at the same pixel to within about 5, with the same arm and stock placement (retail captures from 2026-09-27, vcod re-shot on master `25c44e3` on 2026-10-09). At 16:10 both crop the gun vertically, because `cg_fov` is horizontal (`xmodel-v14-format.md`, "The view fov"); that is retail's look, not a pose bug.

Some idle clips park a part out of sight by translation: `viewmodel_mosinnagantscoped_idle` keys `k98 clip` at (120, -468, -1822) (VERIFIED, frame 0 of the clip). A bounds check over a posed viewmodel mesh has to expect those outliers.

### United Offensive's rigs

United Offensive's newer view rigs, the `w_*_view` gun models on `viewmodel_hands_new` (`gewehr43_mp`'s `w_ge_rfl_gewehr43_view`, `svt40_mp`'s `w_rs_rfl_svt40_view`), key `tag_torso` in every clip, idle, fire, ADS fire and reload alike, at one hip placement: (4.39, -2.92, -3.06) for the gewehr43 and (5.22, -3.41, -2.84) for the svt40 (VERIFIED, every named clip in `weapons/mp/gewehr43_mp` and `svt40_mp`, `pakuo07.pk3`). Their `adsUpAnim` does not close on a sight placement: `w_ge_rfl_gewehr43_view_ADSup` ends at (1.78, 3.27, 2.11) and `w_rs_rfl_svt40_view_ADSup` at (0.22, 4.40, 1.72), left of and above the eye, the gewehr43's barrel pitched 6 degrees up against idle's 9 down (VERIFIED, the clips). The mean of idle's and `adsUpAnim`'s last frame puts the gewehr43's `tag_torso` at (3.08, 0.17, -0.47) and the svt40's at (2.72, 0.50, -0.56), on the view axis like the kar98k's (4.83, -0.26, -0.49), and the barrel level (vcod measurement). So the cgame blends the sight node with the main clip's node at equal weight, per bone, rather than letting one override the other (INFERRED, from that geometry; `uo_cgame_mp_x86.dll` not read). The stock 1.1 rigs cannot tell the two rules apart, since no main clip keys `tag_torso`. `ViewWeapon::pose` in `crates/client/src/viewmodel.rs` blends the two as sibling nodes (`PoseBuffer::blend_nodes`), with idle as the main clip while sighted; `uo_rigs_centre_the_sight_in_the_blend_with_idle` pins the muzzle within a unit of the axis sideways and 2.5 vertically.

## Sampling and the translation-key gotcha

`Track::sample` lerps positions and slerps rotations between the bracketing keys, clamps outside the keyed range, and returns nothing for a channel with no keys. Rotation keys replace the local rotation outright.

A track whose rotation channel has no keys poses the bone at identity, not at whatever an earlier clip left there. VERIFIED, `cod_lnxded` (1.1d): the loader stores a null key pointer for a track with no rotation keys (`0x080b79d7`, the `rot_count == 0` arm), and the sampler at `0x080b8370` tests that pointer at `0x080b8430` and on null adds only the clip weight to the sum's `w` (`0x080b8598`), an identity quaternion at that weight. That the client's sampler in `CoDMP.exe` treats it the same is INFERRED (the same engine code; not read out of the client). The case that shows: `viewmodel_mp44_idle` carries a `tag_weapon` track with no rotation keys (VERIFIED, the clip), and `viewmodel_mp44_pullout` turns `tag_weapon` from 82.5 degrees at frame 0 to identity at frame 12, 0.4 s (VERIFIED). `mp44_mp`'s `raiseTime` is 0.35 (VERIFIED, `weapons/mp/mp44_mp`), so the server leaves the raise at about frame 10.5 with `tag_weapon` still turned 4 to 6 degrees; holding that turn through idle put the gun about 2 units off the hands at the muzzle until the next clip that keys the bone (vcod measurement, `idle_after_a_cut_raise_puts_the_gun_back_in_the_hands` in `crates/client/src/viewmodel.rs`; GitHub issue 5). `PoseBuffer::apply` now poses an empty channel at identity (rotation) or the bind local (translation); a bone the clip has no track for still keeps its local, which is how the `tag_torso`-only sight clips sit under the main clip.

Translation keys are offsets from the bind local position, not absolute locals. The shipped player clips prove it: they key `bip01 pelvis` with small values (-2.2 idle, -22.3 crouched, -31.0 prone) against a bind local z of 37.23. Read as absolutes those collapse the whole stance about 39 units into the ground; read as offsets they put `bip01 l/r toe0` within about 1 unit of z = 0 across `pb_stand_alert`, `pb_combatrun_forward_loop`, `pb_crouch_alert`, `pb_crouch_run_forward` and `pb_prone_aim`, four visibly different stances. Every other translation-keyed player bone (spine, clavicles, toe0, fingers) reads the same way, as fractions of the bone's bind length. Viewmodel rigs hide this because their bind local positions are all exactly zero (checked across every shipped `viewmodel_*` model; the viewhands quirk in the xmodel doc is why), so offset and absolute are the same number there, which is how the absolute reading survived the viewmodel work. `PoseBuffer::apply` does `local_pos = bind_pos + key`.

### Rotation keys as the engine rebuilds them

VERIFIED, `cod_lnxded` (the 1.1d Linux server): the anim loader rebuilds a full key's `w` in 0x080c2598 as `0x3fff0001 - (x^2 + y^2 + z^2)` in integers (`0x3fff0001` is 32767 squared), 0 when that is under 1, else the square root plus 0.5 (`0x080db988`) truncated to a short; a z-only key's in 0x080c264c the same way off `z` alone. The loader 0x080b7160 then negates a key whose dot product with the one before it is negative (the `< 0` test in the loop after the call that returns to 0x080b7e70), so a track's keys run continuously; a bit per track negates its first key before that (the `uVar26 & 1` arms after the key rebuilds that return to 0x080b7a96 and 0x080b7dc5). VERIFIED, same function: that bit is bitset A, the first after the header (`puVar16`, the read pointer before the copy at 0x080b7838 takes bitset B, the z-only bits, into `__dest`). So a track's keys carry the sign the file chose, and the sampler and the node walk add them as signed (cod11-movers.md 15, "Torso over legs"). VERIFIED, vcod measurement, `legs_ab`'s torso gate: without the bitset and with every sum aligned to its first contributor, ours' `bip01 l hand` turned 178 degrees off retail's through each reload's blend-out; with it, 0.13 at worst. vcod: `xanim::parse`. The sampler 0x080b8370 lerps the four shorts and scales by the clip weight over 32767 (`0x080db1ec`) into a per-bone sum. INFERRED: `w` is quantized to 1/32767, so a near-half-turn key (`w` a few hundred thousandths) rebuilds up to a degree off `sqrt(1 - |v|^2)` in floats. VERIFIED, vcod measurement, `crates/server/tests/legs_ab.rs`: with the float rebuild the right foot of a steady run read 0.39 units and 0.73 degrees off retail's on the rows that sample between keys 6 and 7 of `pb_combatrun_forward_loop`, whose `bip01 r thigh` keys there have `w` 0.007 and 0.010; with the integer rebuild the worst steady row over the whole walk is 0.03 units and 0.13 degrees. vcod: `xanim::dequant`.

Notetracks are parsed for EOF validation and have no runtime consumer yet.
