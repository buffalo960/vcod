# CoD 1.1 shader scripts

What CoD's material scripts contain, what vcod implements of them, and where
the implementation deliberately diverges. Evidence rules as everywhere in this
directory: VERIFIED means I measured it against the stock assets or saw it in
the running client; INFERRED means read off the RTCW sources or asset shape and
labelled so. RTCW references are `private/reference/RTCW-MP/src/renderer/*.c`
(the closest public ancestor of the CoD 1.1 renderer); vcod references name the
file and line on `feature/shader-scripts`. The vcod-side parser and runtime
math live in `crates/common/src/shader.rs`, application in
`crates/client/src/renderer.rs` and `crates/client/src/sky.rs`.

## 1. What shader scripts are and where they live

CoD 1.1 keeps the Q3/RTCW material-script system: a BSP soup material name is
looked up in a shader library parsed from `scripts/*.shader` blocks across all
mounted paks; a name with no block falls back to being used as a plain image
path. Block syntax is Q3's `{ keyword args ... { stage } }` grammar. Two
CoD-specific placements matter:

- World materials resolve from `scripts/*.shader` in `main/` paks; every
  authored stock block sits in pak4, pak5, pak8 or pak9 of the Deluxe install
  (VERIFIED by census; see §2).
- Effect shaders for the fx system live in `fxshaders/` inside `pak5.pk3`, not
  `scripts/`, and are almost all additive (`blendfunc gl_one gl_one`). This was
  already noted in AGENTS.md; the fx path consumes them as texture sources.
- United Offensive adds its own scripts under `uo/`, e.g. the MP sky blocks in
  `uo/pakuo01.pk3 : scripts/gmi_sky.shader` (VERIFIED: I read the
  `textures/skies/noville` block straight out of that archive).

vcod parses every script entry into `ShaderLib` at load
(`shader.rs` `ShaderLib::load`). A block whose name matches nothing drawn never
costs more than memory.

## 2. Census

Method: parse every `.shader` entry from the stock `pak[0-9].pk3` of my Deluxe
1.5 install (assets identical between 1.1 and 1.5), then cross-reference block
names against the soup-material set of every map in those paks. The throwaway
script I used for the first pass is gone on purpose; the durable form of this
census is `crates/common/tests/shader_corpus.rs`, which re-measures the pins on
every run (needs `COD_DIR`) and fails when parser changes move them. Like the
other census tests it scopes to stock paks only, because live-server downloads
drop third-party `zzz_*.pk3` files into `main/`.

Headline numbers, measured 2026-08-26 on the Deluxe install (all VERIFIED):

| measure | value |
|---|---|
| distinct authored blocks in stock paks | 604 |
| distinct world material names across stock maps | 2441 |
| of those with an authored block | 301 (rest are implicit image paths) |
| world-referenced block instances, summed over maps | 302 |

Stage-keyword counts over the stages those world blocks expand to (a block has
one or more stages):

| keyword | share / count |
|---|---|
| rgbGen | 92.9% — exactVertex 180, vertex 88, identity 21, identityLighting 15, const 12, constLighting 11, wave 2 |
| blendFunc | 65.3% |
| alphaGen | 37.3% — vertex 111, const 19 |
| tcMod | 40.7% — scroll 166, scale 93, turb 57, transform 9, rotate 1, stretch 0 |
| tcGen | 26% — vector 90; nv_* only behind `requires` gates; environment ungated in mp_ship's six door and window blocks (§3) and stalin_1wet |
| depthWrite | 18.4% |
| alphaFunc | 18.6% — every occurrence GE128 |
| animMap | one block |

blendFunc pairs in world blocks: src_alpha/one_minus_src_alpha 141,
one/one 51, src_alpha/one 18, dst_color/zero 17, plus the shorthands
(`add`, `filter`, `blend`). `nextbundle` appears in roughly 140 world blocks —
water `$lightmap` bundles, terrain blends, foliage, facades, windows, flags —
so two-bundle stages are core CoD grammar, not an exotic corner.

A `$lightmap` bundle must carry the same overbright as the implicit
texture-times-lightmap path, or coplanar surfaces split in brightness:
`scripts/terrain.shader` overlays (`rgbGen exactVertex` + `alphagen vertex`
over `map $lightmap` `blendFunc filter`, polygonOffset) sit on base ground
soups whose material is the raw image path with no script at all, and the
pair must shade identically for the vertex-alpha cross-fade to read as one
surface (mp_brecourt: 19 staged overlay soups over 65 implicit base soups).
vcod applies its x2 overbright and fx-light term to `$lightmap` bundles in
`fs_stage` for this reason (`shader.wgsl`).
A stage with no `$lightmap` bundle takes that x2 itself unless it, or a
later `$lightmap` stage, multiplies the framebuffer (`cod11-gamma.md`
section 5).

Sky: every stock MP map carries at least one sky-parms material among its soup
materials (pinned by the corpus test's skyless assertion), and every stock
skyParms line is `skyParms env/<name> 512 -`: farbox plus cloud dome at height
512, never an inner box.

## 3. Grammar subset implemented

The parser accepts the keywords below; anything else inside a stage warns once
per (block, message) via `WarnSet` (`shader.rs`) and is skipped. RTCW line
anchors from `tr_shader.c` unless noted.

| token | RTCW anchor | vcod notes |
|---|---|---|
| map / clampmap | ParseStage :529 | image resolved through `assets::resolve_bundle_image` (`assets.rs`); the load flags `noopt`, `only16bit`, `only32bit` before the name are skipped (below) |
| animMap | ParseStage :529 | frame list per bundle |
| nextbundle | no RTCW keyword (CoD-only) | opens bundle 1 of the same stage (`shader.rs` nextbundle arm) |
| blendFunc | ParseStage :529 | full factor pair plus shorthands add/filter/blend |
| alphaFunc GE128 | ParseStage :529 | cutout band; GE128 is the only form the corpus uses |
| rgbGen / alphaGen forms | ParseStage :529 | exactVertex, vertex, identity, identityLighting, const(Lighting), wave; alphaGen vertex/const |
| tcMod scroll/scale/turb/transform/rotate/stretch | ParseTexMod :321 | runtime math in §4 |
| tcGen vector / lightmap / environment | ParseStage :529 | vector basis dots world position in the VS; environment reflects the view (below); other forms drop the stage with a warning |
| depthFunc lequal / equal / always | no RTCW keyword (CoD-only) | world stages still draw `LessEqual`; entity and viewmodel stages honour it (below) |
| depthWrite | ParseStage :529 | explicit keyword beats the blend default (`depth_write()` in `shader.rs`) |
| cull / polygonOffset / nopicmip / nomipmaps / surfaceParm | ParseShader :1367 | surfaceParm sets water/sky flags among others |
| sort | sort-name table tr_shader.c :1224-1242 | mapping in §5 |
| skyParms | ParseSkyParms :1152 | farbox + cloud dome; see §6 |
| sunfile | no RTCW anchor (CoD-only) | argument stored on the block (`Shader::sunfile`), resolved to `scripts/<name>.sun` and drawn as the sun disc (§8) |
| fogvars / skyfogvars / waterfogvars / nofog / tesssize / light / entityMergable / qer_* / q3map_* | ParseShader :1367 | consumed as known tokens; script fog values stay inert, MP fog rides configstring 12 (§8) |

CoD-only stage keywords read out of CoDMP.exe 1.1's stage parser
(0x4f8390):

- VERIFIED, 0x4f8990..0x4f89f9: `depthfunc` (string 0x547fac) takes
  `lequal` (0x547f20) for GL state 0, `equal` (0x547f18) for 0x20000 and
  `always` (0x547f10) for 0x40000; anything else warns "unknown depthfunc"
  (0x547ee0), and the keyword is refused inside a `nextbundle` (0x547f6c).
  The GL state setter 0x4d57b0 turns 0x20000 into `GL_EQUAL` and 0x40000
  into `GL_ALWAYS` (`cod11-light-grid-and-leaf-lights.md` section 13).
- VERIFIED, 0x4f9838..0x4f9859: `tcGen environment` (0x5478a8) stores
  texgen 4 in the bundle (`+0xa0`) and ORs 2 into the shader word at
  `0x11e67c8`; `lightmap` (0x54789c) stores 2 and `vector` (0x55b358) 6
  with its two basis vectors at `+0xa4` and `+0xb0`, Q3's `texCoordGen_t`
  numbering.
- VERIFIED, 0x514d40 (called from the texgen switch at 0x500520): per
  vertex, `v = normalize(viewOrigin - xyz)` (0x42dd30, the 0x5f3759df
  reciprocal square root with one Newton step), `d = n . v`,
  `r = 2 d n - v`, then `s = (r.y + 1) * 0.5` (constants 1.0 at 0x568e60,
  0.5 at 0x568e70) and `t = 0.5 - r.z * 0.5`: RTCW's
  RB_CalcEnvironmentTexCoords unchanged. `viewOrigin` is read at
  0x16d8dd8, `+0x30` of the orientation at 0x16d8da8 whose model matrix
  sits at `+0x3c` (0x16d8de4, loaded by the surface loop at 0x4d6ad5).
  INFERRED from that layout, which is RTCW's `orientationr_t`: the eye and
  the vertices are in the entity's frame, so the reflection turns with the
  model, not with the world.
- VERIFIED, 0x4f35dc..0x4f3626, the image parse: `only16bit` (0x54a5d4),
  `only32bit` (0x54a5c8) and `noopt` (0x54a5c0) are flags before the image
  name (`noopt` ORs 0x100 into the image flags), so `map noopt
  $texturename` in `viewhands.stype` names `$texturename`.

- VERIFIED, 0x4f8a23..0x4f8a46: `perLight` (0x547e98) ORs 0x80 into the
  stage's flag word (`+0x00`; `detail` is 0x2 there) and is refused inside
  a `nextbundle` (0x547e58).
- VERIFIED, 0x500b30, the base stage loop: it walks the shader's stage
  pointers (array at 0x19bffc4, up to eight, stopping at a null) and draws
  a stage (0x5008a0) only when that flag byte's top bit is clear. A
  `perLight` stage never draws in the base pass.
- VERIFIED, 0x500d3a..0x500e19, the stage iterator's tail: after the base
  loop it calls the dlight pass 0x500a10 when the batch's dlight mask
  (0x197ffa0) is non-zero, the shader's sort (`+0x58`) is at most 5.0
  (0x5690f4; `fcomp`, `test ah,0x41`, `jp`) and its word at `+0x5c` has
  neither bit of 0x20004. Otherwise, when the shader's OR of stage flags
  (`+0x54`) has 0x80 and the current light list (pointer 0x16d8e5c) holds
  lights (count `+0xcc`, pairs from `+0xd0`), every `perLight` stage draws
  once per light whose first word is not 8. INFERRED: that list is the
  model light pick's (`cod11-light-grid-and-leaf-lights.md` sections 6
  and 13), so a blended surface (sort above 5) lights per listed light,
  not per dlight.
- VERIFIED, 0x500a10, the dlight pass: for each of the frame's world
  dlights (count 0x16d8b1c, 0x88-byte records at 0x16d8b24) whose bit is
  set in the mask, 0x4ff690 projects the light onto the batch and returns
  the touched triangles. When `+0x54` has 0x80, every `perLight` stage
  draws over those triangles, once per light; otherwise the stage of
  `dlightshader` (registered at 0x4fdb0b, `scripts/gfx.shader`: `$dlight`,
  `blendFunc GL_DST_COLOR GL_ONE`, `depthFunc equal`) draws instead.
- VERIFIED, 0x4fb3c0: a shader left with one stage that passes its checks
  (two bundles, a blend state among 0, 0x12, 0x22, 0x25, 0x65, both
  bundles' `+0x94` at 0x2100) gets a second stage copied from the first,
  bundle 1 bound to the dlight image (0x16c4ddc, built at 0x4f0172 with
  `r_dlightRadius` / `r_dlightShift`), and flag 0x80. INFERRED: so a plain
  texture-times-lightmap surface lights up per dlight through its own
  texture.
- VERIFIED, string search of the stock `maps/MP/*.bsp` against the stock
  blocks (2026-10-12): 23 blocks carry `perLight` (`jeff.shader`,
  `surfacelight.shader`); mp_ship references eight (the bridge windows,
  ship doors and the two deck flags), mp_chateau `litwindow1aext`,
  mp_powcamp `barrackswindow3`.

vcod drops a `perLight` stage in the parser (`StageBuf::finish`), so it
never draws. There is no per-dlight pass: the world's fx-light term on its
lightmap bundles (`fx_light_term` in `shader.wgsl`) stands in for both the
`perLight` stages and `dlightshader`. Before 2026-10-12 the stage drew
every frame with the blob at bundle 1's coordinates, so mp_ship's doors
and windows read too bright.

vcod: `TcGen::Environment` and `DepthFunc` in `shader.rs`. The env UV is
computed in the vertex shader (`env_uv` in `shader.wgsl`,
`dynamic_model.wgsl`, `viewmodel.wgsl`) with the eye and normal turned
into the model's frame (the moved brush model's pose, the entity's axes,
the viewmodel's motion transform), before the bundle's tcMods.

VERIFIED, string search of the 12 stock `maps/MP/*.bsp` against the stock
script blocks (2026-10-11): one map references a world block with an
ungated `tcGen environment`, mp_ship, six of them
(`textures/battleship/bridgewindowbroken`, `bridgewindow2broken`,
`shipdoor3a`, `shipdoor3b_trans`, `textures/battleship2a/shipdoor2a`,
`shipdoor2a_trans`): an opaque `environmap_1day` stage under the blended
door or window texture with its lightmap. None references a block with
`depthFunc`; its stype users are `objective_masked` and `pickup_masked`
(`depthfunc equal` on their glow stages), so the world pipelines ignore
it. The env stype users are `metal_env` and `wood_env`.

Classification and ordering: `Shader::classify_stage` sorts each stage into
Opaque / Blend / Additive (dst factor One or SrcAlphaSaturate means additive),
and the renderer buckets draws into five emission bands - opaque (incl.
alpha-test cutouts), coplanar decal, see-through, back-to-front blends sorted
by per-batch centroid depth, additives last
(`renderer.rs` `stage_draw_facts`, bands comment and the draw-loop band walk).
Per-stage state rides to the GPU in 176-byte `StageParams` slots
(`renderer.rs`; size pinned by a const assert), one bind group per stage, with
animated stages re-evaluated per frame.

Ruling on blend-sort granularity, recorded so nobody re-litigates it: sorting
keys on per-batch centroids (material x lightmap), not per-soup, so translucent
soups sharing a batch never reorder against each other. The numeric sorts
between SEE_THROUGH and BLEND0 (water 8.75, banner 6, underwater 8) collapse
into one unsorted seethrough band. Fly-throughs on mp_harbor and noville showed
no artifacts from that collapse; I revisit only if overlapping same-material
translucency ever does.

One sampler pair serves every stage (`stage_bind_groups`, `renderer.rs`), so
when either bundle of a stage is clamped both currently sample with
ClampToEdge and a repeat bundle-0 + clamp bundle-1 stage would wrap its
bundle-0 incorrectly; zero known corpus hits.

## 4. Runtime math

- Wave forms index a 1024-entry sin table built over SIZE-1 degrees of arc, so
  entry 256 is sin(90.088 deg), not exactly 1: `tr_init.c` :1150 builds
  `tr.sinTable` with `i * 360 / (FUNCTABLE_SIZE - 1)`, i.e. the /1023 divisor;
  vcod mirrors it in a const table (`build_sin_table`, `shader.rs`) and pins it
  against libm over the full range in a unit test.
- Index rounding is nearest-even: WAVEVALUE (`tr_shade_calc.c` :34) feeds
  `myftol`, which is x87 fistp on the Windows client and rounds to nearest, so
  511.9 indexes 512 where a truncating cast would take 511. Under
  linux-i386 non-asm builds myftol compiles to plain truncation, so table
  lookups are config-dependent in retail itself. vcod uses
  `round_ties_even` (`wave_value`, `shader.rs`), choosing the Windows-client
  behaviour.
- tcMod scroll clamps to the fractional part before adding
  (RB_CalcScrollTexCoords, `tr_shade_calc.c` :1041); scale is RB_CalcScale...:
  1028; transform is RB_CalcTransformTexCoords :1062; stretch is
  RB_CalcStretchTexCoords :88. All affine, so vcod composes them into one
  2x3 matrix per bundle (`bundle_affine`, `shader.rs`).
- tcMod turb is not affine: s rides `(x + z)` and t rides y, both divided by
  1024 (= 1/128 * 0.125 in the C), offset by phase + time*freq, scaled by
  amplitude through the sin table (RB_CalcTurbulentTexCoords,
  `tr_shade_calc.c` :1009). vcod ships `[amp0, now0, amp1, now1]` in
  StageParams and does `sin(worldpos_axis/1024 + now)` in the VS
  (`shader.wgsl`). An earlier draft had the s-axis wrong; the corpus-era
  screenshots caught it and it now matches the C.

## 5. Sort tokens and the draw order

`map_sort_token` (`shader.rs`) mirrors the RTCW numeric enum
(`tr_local.h` :110-135): portal 1, sky 2, opaque 3, decal 4, seethrough 5,
banner 6, underwater 8, blend0 9, blend1/additive 10, almostnearest 14,
nearest 15. Numeric tokens parse directly and win, as in retail.

CoD-only names found in world blocks and their mapping (counts from the §2
census): water 27 and ocean 2 → 8.75 (between UNDERWATER and BLEND0, where
retail's water plane belongs); outer 9 (+outerblend) → BLEND0; inner 4
(+innerblend, additive 4) → BLEND1; seethrough 2 → SEE_THROUGH. Blocks without
an explicit sort get the FinishShader defaults (`tr_shader.c` :2180): sky →
SS_ENVIRONMENT, then polygonOffset → SS_DECAL, then stage-0 blend with depth
write → SS_SEE_THROUGH, without → SS_BLEND0, no active stage → SS_OPAQUE;
`Shader::sort_value` copies that order exactly.

The draw list, VERIFIED from CoDMP.exe 1.1:

- Every draw surface, world or entity, is one 8-byte entry `(key,
  surface)` appended at 0x16c57b4 (count 0x16c57b0, at most 0x10000).
  The world's (0x4d4fb0, through 0x4e1760) and an xmodel's (0x50db20,
  from the entity add 0x50e3e0) build the key the same way:
  `((shader+0x48 | vb << 12) << 18) | entity << 8 | low bits`, the
  entity number shifted at 0x16c5348 (`0x3fe00`, entity `0x3fe`, for
  the world: 0x4e4c7d), bit 2 set for xmodel surfaces.
- 0x4d5070 sorts the entries by the whole key, unsigned (the quicksort at
  0x4d4e70, called at 0x4d50a8); the draw loop 0x4d6710 decodes the
  shader from bits 18..29 and the entity from bits 8..17.
- `shader+0x48` is the sorted index R_SortNewShader (0x4fb790) assigns: it
  inserts the new shader after every shader whose sort (the float at
  `+0x58`) is lower, ties broken by the first stage's image numbers
  (`image+0x58`, bundle 0 then bundle 1), and bumps the index of every
  shader it passes.
- `vb` (bits 30-31) is the surface's vertex buffer kind:
  `0x16d8868` for an xmodel, set by 0x4b8420 to 3 and to 1 or 2 only on
  the NV_vertex_array_range path without `GL_ARB_vertex_buffer_object`.
  INFERRED: with VBOs every surface carries 3, so the order is the shader
  sort first, then the shader, then the entity, and within one shader the
  entities (lower numbers) draw before the world (`0x3fe`).

So a blended entity surface is not depth-sorted: it draws in its
material's sort among the world's surfaces, and an opaque entity draws
before the world's translucent ones. vcod (`ent_phase`, `sorted_ent_draws`
in `renderer.rs`): the world's draw list keeps its five bands (§3), and
the live entities draw after the band their material's sort names, opaque
(sort up to 3) after the opaque band, decal and see-through (below 9)
after those, the rest after the blend band and before the world's
additives. Within that, entity surfaces sort by material sort, then the
material's load order (standing in for the sorted index), then instance
order; each surface draws its stages back to back.

The viewmodel. VERIFIED, cgame `0x30036cf0` builds the gun's refEntity
with `reType` 1 and `renderfx` 0x8c (`0x30036e27`, `0x30036e2f`) and
adds it through syscall `0x3d`; `0x300371f0` is called at `0x30033e04`,
after the packet entities (`0x30033df5`, `0x30033dfa`). VERIFIED,
CoDMP.exe's draw loop `0x4d6710` switches on the surface entity's
`renderfx & 0x18` (`0x4d69bd`, table `0x4d6c78`/`0x4d6c88`): 0 sets the
depth range 0..1 (`0x4d6a56`), 8 loads the projection at `0x16d8cd0` and
sets 0..0.2 (`0x4d6a5d`-`0x4d6a83`), 0x10 and 0x18 set 0..0.5
(`0x4d6a8a`); leaving 8 restores the projection at `0x16d8c90`
(`0x4d6a98`). INFERRED: the gun has no list of its own; its surfaces sort
by material with the world's and the entities', after the entities within
one shader (a later entity number), and draw in the front fifth of the
depth range with the depth-hack projection (`xmodel-v14-format.md`, "The
view fov"). So an opaque gun draws before the world's translucent
surfaces, which then fail the depth test against it. vcod: the
viewmodel's surfaces draw after each band's entities, phase by phase as
`ent_phase` names, in the viewport depth range 0..0.2; the fx quads,
still drawn after every band, test against the gun's depth.

## 6. Sky

Stock skies are always farbox + cloud dome: `skyParms env/<name> 512 -`, no
inner box anywhere in the corpus (§2). The geometry is transcribed from
`tr_sky.c`:

- Cube axis → env-image suffix comes from ParseSkyParms loading
  {rt, bk, lf, ft, up, dn} into outerbox[0..5] while DrawSkyBox renders axis i
  with outerbox[sky_texorder[i]] and sky_texorder is {0,2,1,3,4,5}
  (`tr_sky.c` :374), giving +x rt, -x lf, +y bk, -y ft, +z up, -z dn
  (`AXIS_SUFFIX`, `sky.rs`).
- Box distance is zFar/1.75 ("div sqrt(3)") with a 2*zNear floor so it never
  near-clips (`box_size`, `sky.rs`).
- Cloud UVs come verbatim from R_InitSkyTexCoords (`tr_sky.c` :749,
  radiusWorld 4096): intersect the ray with the sphere of radius
  radiusWorld+height centred at (0,0,-radiusWorld), re-centre, normalise, acos
  x and y (`cloud_uv`, `sky.rs`). The raw radians are the UVs; stage tcMods
  scale them onto the repeating cloud texture.
- The dome is five cube faces (FillCloudBox skips the bottom), grid indices in
  FillCloudySkySide's pattern (`tr_sky.c` :571, index loop :597).
- As in RB_StageIteratorSky (`tr_sky.c` :994), if the farbox's first side
  image fails to load the whole box is skipped; the dome still draws through
  the stage path (`renderer.rs` sky-pass block).
- Which block owns the sky pass: highest soup reference count wins, ties to the
  alphabetically first name (`pick_sky`, `sky.rs`).

noville carries the only prominent scrolling cloud stage in the corpus:
`map textures/skies/noville_clouds.tga`, src_alpha blending, tcMod scroll
0.004 0.004 then scale 3 3, gated `requires cvar scr_gmi_fast == 0`
(VERIFIED: read out of `uo/pakuo01.pk3 : scripts/gmi_sky.shader`,
`textures/skies/noville`).

## 7. CoD-only grammar and requires-gating

Four pieces of the corpus have no RTCW parser counterpart:

- `nextbundle` (two-bundle stages), everywhere: §2 numbers.
- `requires <atom> [|| atom]...` stage gating. Atoms are GL extension
  identifiers, optionally with a relational operator and number, or
  `cvar <name> <op> <value>`; a top-level requires attaches to the following
  stage only. The census hits are NV/ATI hardware paths: GL_NV_texture_shader,
  GL_NV_register_combiners, GL_ATI_fragment_shader, texEnvCombine variants,
  `waterMap`, cubemap nextbundles, tcGen
  environment / nv_dot_product_reflect_cube_map_eye_from_qs_*, sunHalfAngle -
  all confined to requires-gated stages except the ungated `tcGen
  environment` of stalin_1wet and mp_ship's door and window blocks, which
  vcod draws since 2026-10-11 (§3).
- CoD sort names (§5).
- `sunfile <map>` on sky blocks.

vcod evaluates gates against a fixed low-spec profile: the six NV/ATI/cubemap
extensions count absent (`CAPS_ABSENT`, `shader.rs`), GL_MAX_TEXTURE_UNITS_ARB
is 4, `sys_cpuMHz >= N` is false and other cvars true. That drops the
hardware-specific stages and keeps the fallback ones, which is what retail did
on machines without the extensions - the gated stages exist precisely because
the ungated fallback had to render the surface. A failed gate skips the stage
body straight to its closing brace, so hw-path fields (register combiner setup
and friends) are never parsed at all. Unit tests pin each rule
(`shader.rs` tests around the `eval_requires` machinery). The profile choice is
INFERRED-from-assets rather than disassembled; retail's exact evaluation order
was never dumped, but the kept/dropped outcome on stock assets is deterministic
under it and matches the fallback shapes.

## 8. Deliberately not implemented

Each omission with its census justification:

- NV/ATI hardware-path stage contents (`waterMap`, register combiners,
  nv_texshader tcGens, sunHalfAngle). Every occurrence but stalin_1wet sits
  behind a `requires` gate this profile fails, so dropping them reproduces
  retail-on-fallback-hardware. The ungated `tcGen environment` stages
  (stalin_1wet, mp_ship's doors and windows) draw (§3).
- `sunfile` sun disc: rendered since 2026-08-26. The token names
  `scripts/<name>.sun` — a cvar dump (CoDMP.exe loads it via
  `"scripts/%s.sun"`; cvars `r_sunsprite_*` registered at CoDMP.exe
  `0x54cc10`/`0x54cbfc`), of which vcod consumes `r_sunsprite_shader`
  ("0" disables the disc, as mp_harbor.sun does) and `r_sunsprite_size`;
  the flare/glare/blind keys are ignored. The disc direction is not in the
  `.sun` file: it is the worldspawn `sundirection` key (mp_ship.bsp:
  `-45 110 0`, alongside `suncolor`/`sunlight`). vcod draws an additive
  billboard at farbox distance when both a sprite image and the direction
  exist; maps whose active sky block has no sunfile get no disc, matching
  retail. Flare/glare/blind screen effects remain unimplemented.
- `deformVertexes wave`: implemented 2026-08-26 after the ocean blocks made
  mp_ship's water read as a flat sheet next to retail. Only the `wave` form
  exists in a world block (mp_ship-style ocean planes); the BSP already
  ships those surfaces subdivided (one 1052-vertex soup on mp_ship), so the
  displacement runs in the vertex shader with no CPU tessellation:
  `pos += normal * (base + amp * sin(2pi * (phase + t*rate + (x+y+z)/div)))`
  (RTCW-MP tr_shade_calc.c RB_CalcDeformVertexes). Other deform kinds
  consume their args and stay inert.
- Fog: the script keywords stay inert, but stock MP fog does not come from
  them. `fogvars` / `skyfogvars` occur only in SP sky blocks (pak4/pak9
  `sky.shader`), `waterfogvars` nowhere at all; `nofog` sits only on fx/gfx
  blocks, which vcod does not fog. The fog every MP map shows is set by gsc
  (`setCullFog` / `setExpFog`, e.g. `mp_ship.gsc`) and reaches the client as
  configstring 12; vcod parses and applies it since 2026-08-26 — wire format
  in docs/protocol-1.1.md, mode rules from RTCW-MP tr_main.c R_SetFog.
  The exp-fog density rides the wire raw: retail's mp_chateau `re` gamestate
  carries `0 1 1e-05 0 0 0 0` for `setExpFog(0.00001, 0, 0, 0, 0)`
  (`crates/server/tests/fixtures/configstrings/mp_chateau-re.txt`, VERIFIED).
  Whether the client adds RTCW's +0.1 before `R_SetFog` is a client question
  this capture does not answer.
- Missing images bind retail's `*default`: 16x16 of RGBA (32, 32, 32, 32)
  inside a one-pixel opaque black border. `R_CreateDefaultImage`, CoDMP.exe
  1.1 @ 0x4f0380 (`mov eax,0x20202020` + `rep stos` over 0x400 bytes, then the
  four border runs written RGB 0 / alpha 0xff, registered 16x16 under the
  `.rdata` string `*default` @ 0x54b57c). `R_FindShader` reaches it through the
  `Couldn't find image for shader %s` branch @ 0x4fc7fc, which flags the
  material and lets FinishShader return the default shader. INFERRED from
  decompilation; not yet compared against a retail frame. `assets::default_image`
  matches those bytes, so a material whose file no pak ships reads as a dark
  surface rather than a placeholder pattern (`textures/cod2/egypt_wire_black`
  on the custom map mp_burgundy is the case that motivated it: the map
  references it and ships it in neither of the server's referenced paks).
- `$dlight` bundle images: engine-generated light blobs, never files on disk
  (corpus whitelist in `shader_corpus.rs`, which cites the pak9
  window.shader occurrence). Every stock one sits in a `perLight` stage,
  which vcod drops (section 3); `assets::dlight_blob` stays bound for any
  other `$dlight` bundle. The mp_ship
  flag decks bind 1x1 white for the same reason that
  `textures/battleship/deckflag_np.tga` (flagfore/flagaft's second bundle)
  ships in no stock pak under any extension; retail binds its own default
  image there too, so both stay in the corpus whitelist.
- `clampY` is approximated (warns once), `heightToNormal` ignored: neither
  occurs in a world block of the stock corpus.

## 9. Verification status

- Corpus tests (`crates/common/tests/shader_corpus.rs`, needs `COD_DIR`):
  >500-block library pin, 250..350 world-reference band, the every-MP-map-has-a-sky
  pin, and the assertion that every stage image of every authored MP material
  resolves through the renderer's probe chain. That last one earned its keep
  during development: it caught 13 paths scripted as .tga but shipped as .dds,
  fixed by routing resolution through the shared `assets::resolve_bundle_image`.
- Unit tests cover the wave-table ties-even rounding, the sin table against
  libm over the full range, turb axes, requires gating rules, sort mapping and
  sky geometry (zenith UV = pi/2, bottom face omitted, box spans).
- Live fly-throughs, VERIFIED by me on the Deluxe install: mp_pavlov (night
  sky), mp_harbor, mp_bocage (overcast hedgerow sky, fence and foliage
  cutouts, terrain detail blends), noville (dusk cloud dome, scrolling cloud
  stage visible), burnville. Zero missing textures on stock main maps; 94-145 fps
  during the passes.
- PENDING HUMAN REVIEW: pixel-truth against the retail renderer. Everything
  above proves the surfaces render as authored; only side-by-side comparison
  with the real game can prove the last percent of blending and sky shading.
- Known divergences, all logged in §8: NV/ATI hw-path stages dropped (retail
  fallback parity), `deformVertexes` forms other than `wave` inert, script fog
  keywords inert (no stock MP map sets fog from a script; the fog every map
  shows arrives in configstring 12 and is rendered), `$dlight` bundles and the
  mp_ship deckflag image have substitutes rather than files, noville's missing textures under `--mod-dir uo` are the
  documented single-mod-dir mounting limitation (its textures ship in main/
  pak0/pak1), turb axis follows tr_shade_calc.c.
