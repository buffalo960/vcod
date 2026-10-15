# `.efx` particle-script grammar census

Exhaustive census of every `.efx` file shipped in CoD 1.5 Deluxe's `main/pak*.pk3`
archives. Assets are version-independent with 1.1, see
`docs/research/cod11-events-and-fx.md` section 8 (the "events doc"
below). This is the grammar vcod's `.efx` parser (`crates/client/src/fx/efx.rs`)
encodes. Every claim is backed by occurrence counts or a specific file, not by
memory of the format from other id Tech or IW engine games.

## 0. Corpus

Unzip the `fx/` tree of every `main/pak*.pk3` into a scratch directory:

```
for p in main/pak{0,1,2,3,4,5,6,8,9,a,b}.pk3; do unzip -o -q "$p" 'fx/*' -d "$S"; done
find "$S/fx" -name '*.efx' | wc -l   # 460
```

460 `.efx` files, all from `pak5.pk3` (459) and `pak9.pk3` (1). Every other
`main/pak*.pk3` (0,1,2,3,4,6,8,a,b) has no `fx/` entries at all. The five
`localized_english_pak{0,1,2,3,5}.pk3` archives contain zero `.efx` files
(`unzip -l | grep -c '\.efx$'`), so localization does not change the census.
`fx/iw_impacts.csv` (section 6) came out of the same `pak5.pk3` extraction. No
case-insensitive duplicate paths exist (`find | tr lower | sort | uniq -d` is
empty), no file is zero bytes, and all 460 parse under the grammar below.

All files use CRLF line endings and tabs for indentation. Verified: no file has a
bare `\n`-only line, no file uses spaces for the base indent level. Every
extraction script below strips the trailing `\r` before matching. A naive
`grep -E '...$'` against these files matches nothing because `$` does not cross
the `\r`.

## 1. Block type keywords

An emitter block is a bare identifier at column 0 (no leading whitespace)
immediately followed by a line that is just `{`. I extracted them with an awk
script that tracks the pending identifier and prints it once the matching `{`
is seen (`/^[A-Za-z]+[ \t]*$/` sets a pending name, `/^\{/` prints and clears
it), run with `\r` stripped first. 10 distinct top-level block types, 1261
blocks total across 460 files (avg 2.74 blocks/file):

| Block type | Count |
|---|---|
| `Particle` | 712 |
| `Emitter` | 162 |
| `FxRunner` | 116 |
| `Light` | 71 |
| `Tail` | 66 |
| `OrientedParticle` | 49 |
| `Decal` | 34 |
| `Line` | 31 |
| `Sound` | 12 |
| `Cylinder` | 8 |

A single `.efx` file is a flat sequence of these blocks with no nesting between
top-level blocks. `fx/impacts/default_hit.efx` is three `Particle` blocks
followed by one `Decal` block; `fx/explosions/grenade2.efx` is `Particle,
Emitter, Particle, Light, Decal, Particle, FxRunner`. All blocks in a file play
together when the file is triggered. There is no selection logic inside the
file itself.

`FxRunner` and `Sound` are pure triggers. `FxRunner` chains to another `.efx`
path via `playfx` (section 3); `Sound` names a sound alias via `sounds`.
Neither has a `life`, geometry or render key of its own. Grepping every
`FxRunner`/`Sound` block body confirms it: `FxRunner` bodies contain only
`name`, `count`, `cullrange`, `delay`, `height`, `origin`, `playfx`, `radius`,
`spawnFlags`; `Sound` bodies contain only `sounds`.

## 2. Keys per block type, with arity

Extracted with a stateful awk script that tracks the enclosing block type and,
one level down, the enclosing curve sub-block name, so that `end` under `rgb`
is distinguished from `end` under `size`. List blocks (`shaders [ ... ]`,
`models [ ... ]`, `sounds [ ... ]`, `emitfx [ ... ]`, `playfx [ ... ]`,
`impactfx [ ... ]`, `deathfx [ ... ]`) are skipped by tracking `[`/`]` depth.
Their contents are asset paths or sound-alias names, not keys, and a census
that does not skip them reports bogus keys such as `gfx`, `fx`, `xmodel`,
`textures`, `KABOOM`, `glass`, `explo`, `shell` (the leading path segment or
first word of list entries).

Two parser details matter for the counts. Keys must be split on whitespace and
taken as a whole token, since a letters-only regex (`/^[a-zA-Z]+/`) stops at
the first digit and merges `size2` into `size` and `origin2` into `origin`.
With whole-token splitting `size` is 1089 (all of them sub-block openers), and
`size2` (168) and `origin2` (31) are their own keys. Cross-check: `grep -c
'^\t+size[ \t]*\r?$'` across the corpus independently gives 1089.

Two shapes of key line exist:

- Value key: `<key>\t...\t<value>`, a scalar, vector or token-list value on the
  same line.
- Sub-block opener: `<key>` alone on its line, next non-blank line is `{`. Five
  sub-block names exist: `rgb`, `alpha`, `size`, `size2`, `length`. Each
  sub-block's body is one or more of `start`, `end`, `flags`, `parm`, the curve
  grammar (section 2b). `size2` is a second, independent
  `start`/`end`/`flags`/`parm` block with the identical shape to `size`. It only
  appears together with the top-level key `nonUniformScale` (section 4) and
  drives the particle's second scale axis (width vs. height) when uniform
  scaling is off. `fx/atmosphere/rainsplash.efx`'s `Particle` has both `size {
  end 5 7 }` and `size2 { end 12 7 }` plus `nonUniformScale 1`.

### 2a. Top-level value keys (23), arity and units

38 distinct key identifiers exist in the whole grammar: these 23, the 5
curve/sub-block openers `rgb`/`alpha`/`size`/`size2`/`length`, the 7 list keys
in the table below, and `start`/`end`/`parm`, which are catalogued in 2b since
they only ever appear inside a curve sub-block.

Arity is the count of numeric tokens on the value line. `1` = fixed value,
`2` = min/max range (uniform-random per spawn), `3` = a fixed vector (x y z),
`6` = min vec3 / max vec3 (per-component random range). Where a key took more
than one arity across the corpus, both are listed with counts. All of these
appear directly under a top-level block, not inside a curve sub-block, except
where noted.

| Key | Arities seen (count) | Example | Block types it appears in | Unit |
|---|---|---|---|---|
| `life` | 1 (786), 2 (344) | `life 1e+004` | all except `FxRunner`, `Sound` | ms |
| `delay` | 1 (110), 2 (119) | `delay 80 0` | `Particle`, `OrientedParticle`, `Tail`, `Line`, `Cylinder`, `FxRunner` | ms |
| `count` | 1 (416), 2 (288) | `count 2 5` | `Particle`, `OrientedParticle`, `Tail`, `Line`, `Cylinder`, `FxRunner` (never `Emitter`) | particles |
| `cullrange` | 1 (206) | `cullrange 1200` | `Particle`, `Emitter`, `Decal`, `Tail`, `FxRunner` | game units |
| `velocity` | 3 (123), 6 (721) | `velocity 1.19e+004 0 0 1.18e+004 0 0` | `Particle`, `Emitter`, `OrientedParticle`, `Tail` | units/sec |
| `acceleration` | 3 (37), 6 (130) | `acceleration -100 21 32 -120 -42 0` | `Particle`, `Emitter`, `OrientedParticle`, `Tail` | units/sec² |
| `gravity` | 1 (169), 2 (133) | `gravity -300` | `Particle`, `Emitter`, `OrientedParticle`, `Tail` | units/sec² |
| `origin` | 3 (81), 6 (546) | `origin 5 12 0 75 -24 0` | `Particle`, `Emitter`, `OrientedParticle`, `Tail`, `Cylinder`, `FxRunner`, `Light` | game units (offset from trigger point) |
| `origin2` | 3 (8), 6 (23) | `origin2 245 456 433 345 -456 -433` | `Line` only | game units (second endpoint) |
| `radius` | 1 (137), 2 (192) | `radius 1 14` | `Emitter`, `OrientedParticle`, `Tail`, `Cylinder`, `FxRunner` | game units |
| `height` | 1 (63), 2 (90) | `height 50 32` | `Particle`, `OrientedParticle`, `Tail`, `Line`, `Cylinder`, `FxRunner` | game units |
| `rotation` | 1 (5), 2 (665) | `rotation 180 -360` | `Particle`, `Decal`, `OrientedParticle` | degrees (initial roll) |
| `rotationDelta` | 1 (27), 2 (487) | `rotationDelta 1 -2` | `Particle`, `OrientedParticle` | degrees/sec |
| `angle` | 3 (7), 6 (39) | `angle 122 0 0 2 0 0` | `Emitter` only | degrees (initial euler orientation, xmodel debris) |
| `angleDelta` | 6 (65) | `angleDelta 12 0 0 11 0 0` | `Emitter` only | degrees/sec |
| `bounce` | 1 (21), 2 (3) | `bounce 0.5 0.7` | `Emitter`, `Tail` | restitution coefficient, roughly 0 to 1 |
| `density` | 1 (126) | `density 4` | `Emitter` only | game units between two `emitfx` plays (R10) |
| `variance` | 1 (28), 2 (41) | `variance 1 5` | `Emitter` only | game units, the jitter on that spacing (R10) |
| `wind` | 1 (10), 2 (1) | `wind 100` | `Particle` only | unit-less multiplier (paired with `affectedByWind` flag) |
| `nonUniformScale` | 1 (81, always literal `1`) | `nonUniformScale 1` | `Particle`, `Emitter` | boolean toggle, not a magnitude (section 4) |
| `name` | free text (127 to 236 words) | `name Copy of Unnamed Particle 0` | all render block types | none, cosmetic editor label |
| `flags` | token list | `flags usePhysics impactKills useAlpha` | all render block types | none, section 3 |
| `spawnFlags` | token list | `spawnFlags orgOnSphere axisFromSphere` | `Particle`, `Emitter`, `OrientedParticle`, `Tail`, `Line`, `Cylinder`, `Decal` | none, section 3 |

`parm` is a value key too but only ever appears inside a curve sub-block
(arity 1 or 2, wave-modulation parameters, section 2b), so it is listed there.

List keys (value is a `[ ... ]` block of bare strings, one asset or alias path
per line, not numeric):

| Key | Count (openers) | Contents | Block types |
|---|---|---|---|
| `shaders` | 898 | `gfx/...` material paths | `Particle`, `Decal`, `OrientedParticle`, `Tail`, `Line`, `Cylinder` |
| `models` | 69 | `xmodel/...` model paths | `Emitter` |
| `emitfx` | 105 | `.efx` paths (chained effect, continuous) | `Emitter` |
| `playfx` | 116 | `.efx` paths (chained effect, one-shot) | `FxRunner` |
| `impactfx` | 19 | `.efx` paths (played on debris impact) | `Emitter`, `Line`, `Tail` |
| `deathfx` | 16 | `.efx` paths (played when debris/particle dies) | `Particle`, `Line`, `Tail`, `OrientedParticle` |
| `sounds` | 12 | sound alias names, not paths (e.g. `KABOOM`) | `Sound` |

The four chaining keys pair with a `flags` token: `emitfx` with `emitFx`,
`impactfx` with `impactFx`, `deathfx` with `deathFx`. The list parsers set
the flag themselves (R9, R10), and no stock block carries one without the
other. `impactfx` plays on each hit of a physics particle until it rests,
`deathfx` where an element's life runs out (R10). The opener counts above
count lists; R10 counts their entries. Every `FxRunner` block in the corpus
contains nothing but a `playfx` list and the keys listed in section 1.

### 2b. Curve sub-block keys (`rgb`, `alpha`, `size`, `size2`, `length`)

Each of the five sub-block names, when opened, contains up to four keys:
`start`, `end`, `flags`, `parm`. This is the interpolation-over-lifetime
grammar. `start` is the value at spawn, `end` is the value at the end of
`life`, `flags` picks the interpolation mode, `parm` supplies parameters for
the `wave` mode. Arity of `start`/`end` depends on the sub-block.

Counts below are `(start-count/end-count)`; `rgb`'s `3 (242/264)` means
arity-3 `start` was seen 242 times and arity-3 `end` 264 times.

| Sub-block | `start`/`end` arity | Example | Meaning |
|---|---|---|---|
| `rgb` | 3 (242/264) or 6 (90/133) | `start 1 0.9843 0.9412 0.7529 0.7529 0.7608` | fixed RGB triple, or min-RGB/max-RGB (6 = two triples) |
| `alpha` | 1 (156/735) or 2 (161/65) | `start 0.4 0.8` | fixed alpha, or min/max alpha |
| `size` | 1 (399/397) or 2 (363/557) | `end 282 374` | fixed size, or min/max size (random within one axis) |
| `size2` | 1 (85/96) or 2 (26/63) | `start 55 5` | same shape as `size`, second scale axis |
| `length` | 1 (120/182) or 2 (31/23) | `start 4 14` | trail/tail segment length, fixed or min/max |

`flags` inside a curve sub-block is a different vocabulary from the top-level
`flags`/`spawnFlags` (section 3). It picks the interpolation mode: `linear`
(2985 of 3164 curve-flag tokens), `random` (145), `clamp` (22), `wave` (14),
`nonlinear` (7). `parm` (32 total, arity 1 or 2, e.g. `parm 0.1 80`) only
co-occurs with the `wave` flag and supplies the wave's frequency and amplitude.

## 3. `flags` / `spawnFlags` tokens

I split `flags`/`spawnFlags` lines by indentation depth (1 tab = a top-level
block's own `flags`/`spawnFlags` key; 2 tabs = a curve sub-block's `flags`
key, covered in 2b), since the two vocabularies share the name `flags`. 4143
total `flags`/`spawnFlags` lines at any depth, matching the key-census count
for `flags` + `spawnFlags` exactly (3697 + 446).

Top-level `flags` (11 distinct tokens, particle/model behavior). Meanings
marked "inferred" come from the token name and co-occurrence only; the `.efx`
content alone cannot pin them down, and vcod's parser preserves the token
without interpreting it.

| Token | Count | Meaning |
|---|---|---|
| `useAlpha` | 428 | particle honors its `alpha` sub-block curve for vertex alpha |
| `emitFx` | 105 | `Emitter` plays its `emitfx` list every `density` units along its path (R10) |
| `relative` | 96 | the element rides the tag its effect was played on (R10) |
| `usePhysics` | 88 | particle/model is physics-simulated (gravity, collision, `bounce`) rather than simple kinematic |
| `impactKills` | 81 | physics particle is destroyed on first surface collision |
| `useModel` | 69 | `Emitter` spawns `models` (xmodel debris) instead of billboard sprites |
| `impactFx` | 19 | physics particle chains its `impactfx` list on collision |
| `depthHack` | 17 | inferred: renders biased toward the camera, the usual id-engine depth hack for close-up/gun effects |
| `deathFx` | 16 | element plays its `deathfx` list where its `life` runs out (R10) |
| `setShaderTime` | 11 | inferred: resets the shader's animation clock to the particle's spawn time rather than global time |
| `expensivePhysics` | 4 | parsed and ignored by the 1.1 engine (R9) |

`spawnFlags` (10 distinct tokens, spawn-geometry/integration mode; a separate
token namespace from `flags`, never overlapping). Bit values and the engine's
reading of each are in R6.

| Token | Count | Meaning |
|---|---|---|
| `orgOnCylinder` | 214 | spawn point distributed on a cylinder surface (uses `radius`/`height`) rather than uniformly in the `origin` box |
| `axisFromSphere` | 207 | initial velocity direction derived from the spawn point's position on the sphere/cylinder (radial outward), not from the `velocity` vector's own direction |
| `evenDistribution` | 91 | particles in one `count` burst are spaced evenly rather than independently randomized |
| `rgbComponentInterpolation` | 72 | `rgb` curve interpolates each R/G/B channel independently instead of as one blended triple |
| `orgOnSphere` | 53 | spawn point distributed on a sphere surface (uses `radius`) |
| `absoluteAccel` | 42 | `acceleration` is world-space, not relative to spawn direction |
| `cheapOrgCalc` | 30 | skips the rotation of the sampled `origin` into the emitter frame (R6) |
| `absoluteVel` | 22 | `velocity` is world-space, not relative to spawn direction |
| `randrotaroundfwd` | 10 | inferred: random initial rotation around the particle's forward/velocity axis |
| `affectedByWind` | 1 | particle motion is perturbed by the `wind` key/global wind |

## 4. Defaults: what a missing key means

First reasoned from single-purpose effects whose visual result is known (the
working-set impact/explosion effects of section 6, whose required look is
pinned by what a bullet hole, rock chip or explosion has to look like) plus
the corpus-wide presence/absence ratio, then checked against the engine's
template initializer (R7). Rows marked "R3"/"R6"/"R7" are VERIFIED from the
binary; the rest are inferred from the corpus.

| Key / block missing | Default | Evidence |
|---|---|---|
| `alpha` sub-block absent | constant 1.0 (opaque) | of the 900 blocks that can carry `alpha` (`Particle`, `Decal`, `Cylinder`, `Line`, `OrientedParticle`, `Tail`; `Light` never uses it), 830 set it and 70 omit it entirely. `fx/impacts/small_rock.efx`'s 2nd `Particle` has `rgb { flags linear }` with no `start`/`end` at all; a bare interpolation-mode flag with nothing to interpolate shows these curve sub-blocks are optional annotations, not requirements. R7 confirms 1.0. |
| `rgb` sub-block absent | white (1,1,1), i.e. the shader's native texture color, unanimated | of the 971 blocks that can carry `rgb` (same set plus `Light`), 794 set it and 177 omit it entirely. R7 confirms white. |
| `end` present, `start` absent (in a curve) | the curve's per-key default: 0 for `size`/`size2`/`length`, 1.0 for `alpha`, white for `rgb` | `fx/error.efx`'s `size { end 133 parm 4 flags wave }` has no `start`; a spark/dot effect that grows from nothing. R7. |
| `start` present, `end` absent (in a curve) | the same per-curve default `start` falls back to: 0 for `size`/`size2`/`length`, 1.0 for `alpha`, white for `rgb`. A `linear` curve with no `end` ramps to that default, so `size { start 210 flags linear }` fades to nothing. Curves that look constant are constant because they omit `linear`, not because they omit `end`. | `fx/impacts/small_glass.efx`'s `Decal` `size { start 3 4 }` has no flags either, so it is constant. 48 `size` curves in the corpus pair `linear` with a missing `end` and do shrink to 0. R7. |
| `velocity` absent | (0,0,0), particle stays at its spawn point | `fx/impacts/default_hit.efx`'s spark-flash `Particle` (1st block) has no `velocity`, only `rotation`/`rgb`/`size`; `fx/impacts/small_glass.efx`'s `Decal` (decals never carry `velocity`, 0 occurrences across all 34 `Decal` blocks). R7. |
| `gravity` absent | 0 (no downward acceleration) | smoke/dust `Particle` blocks (e.g. `fx/impacts/small_rock.efx`'s dust-layer 3rd block) omit it and visibly drift rather than fall. R7. |
| `acceleration` absent | 0 | 615 of 712 `Particle` blocks omit it. R7. |
| `rotation` absent | 0 (renders unrotated) | 2 of 34 `Decal` blocks omit it. R7. |
| `rotationDelta` absent | 0 (rotation, if any, stays fixed at its initial value) | the majority of `Particle`/`OrientedParticle` blocks that set `rotation` but not `rotationDelta`. R7. |
| `origin` absent | (0,0,0), spawns exactly at the effect's trigger point | every working-set impact/explosion `.efx` (section 6) omits `origin` entirely and plays at the bullet/explosion point, the only correct behavior for an impact decal. R7. |
| `count` absent | 1, a single particle/segment per trigger | no `.efx` sets `count 1` redundantly to compare against an omitted one, and `Decal` (always exactly one decal per hit) never carries the key at all. R7 confirms 1. |
| `nonUniformScale` absent | 0/false; `size2` is unused even if present (never observed: `size2` never appears without `nonUniformScale 1` in the same block) | 81/81 `nonUniformScale` occurrences are the literal value `1`; it is a presence flag encoded as a dummy-valued key, not a magnitude |
| `spawnFlags`/`flags` absent | empty set, every boolean behavior in section 3 off | direct reading of the token-list grammar |
| `cullrange` absent | 0, and 0 means "never culled". Not an engine-wide draw distance. | R7: the template initializer leaves `+0x64` at zero and the effect runner's `if (cullrange == 0)` branch (`0x493a1f`) jumps past the distance check. 206 of the 1090 blocks that can carry it set it. |
| `density`/`variance` (`Emitter`) absent | 0: an `emitfx` emitter with neither plays on every 12 ms step of its path | R7 leaves them zero; R10. They space `emitfx` plays only; an `Emitter` spawns `count` instances like every other element |
| `radius`/`height` absent, but `orgOnSphere`/`orgOnCylinder` set | 1.0 | R7: the template initializer sets both to 1.0, so an unset `radius` distributes points on a unit sphere/cylinder. Not observed in the corpus; every `orgOnSphere`/`orgOnCylinder` block checked sets `radius`. |
| `radius`/`height` set, but neither `orgOnSphere` nor `orgOnCylinder` | both keys are ignored | R6: they are read only inside those two flag branches. `fx/tagged/tracers.efx`'s `radius 45 55` / `height 100 10` do nothing. |
| `delay` absent | 0, spawns immediately, no stagger | the majority of single-segment `Particle` blocks omit it; multi-segment `Tail`/`Cylinder`/`Line` effects, which need staggered emission to look like a continuous trail, set it almost universally. R7. |
| curve `flags` (interpolation mode) absent, both `start` and `end` present | the curve does not animate: it holds `start` for the whole life and `end` is ignored | R3: the evaluators leave the interpolation fraction at 1.0 unless the `linear` bit is set, and `1*start + 0*end == start`. There is no default ramp. |
| `name` absent | no effect, cosmetic editor label only | present on roughly half of `Particle`/`FxRunner`/`Tail` blocks with no visible correlation to rendering |

## 5. Units

- `life`, `delay`: milliseconds. Confirmed by scale plausibility
  across the whole life-duration range. `fx/explosions/shock.efx`'s `Cylinder`
  shockwave ring has `life 211` (0.211 s, right for an instantaneous blast
  ring); `fx/impacts/default_hit.efx`'s `Decal` has `life 8000` (8 s, a bullet
  hole fading out); `fx/explosions/grenade2.efx`'s `Decal` (scorch crater) has
  `life 4e+004` (40 s, a longer-lived scorch mark); `fx/tagged/tracers.efx` (a
  `Tail`) has `life 3700 4300` and `delay 300 100` (a few seconds and a few
  hundred ms of stagger, right for a decorative tracer chain). All of these are
  wrong by orders of magnitude if read as seconds or ticks.
- `velocity`, `acceleration`: units/sec, units/sec². `fx/tagged/tracers.efx`
  sets `velocity 1.19e+004 0 0 1.18e+004 0 0`, i.e. 11900/11800, matching
  CoD's bullet-speed constant (~1.19e4). That only makes physical sense if
  `velocity` is in game units per second: CoD's game unit is roughly an inch,
  and ~11900 in/sec is ~990 ft/sec, in range for a rifle-caliber muzzle
  velocity. `gravity -300` (a typical `Particle`) is consistent with the same
  unit scale for acceleration.
- `rotation`, `rotationDelta`, `angle`, `angleDelta`: degrees, degrees/sec.
  Values cluster in `[-360, 360]` for `rotation`/`angle` (a full turn), and
  the `*Delta` companions are consistent with degrees/sec once divided by
  typical multi-hundred-ms lifetimes. A radian reading fails: `rotation 360
  -360` would be ±22,900° under 2π radians.
- `radius`, `height`, `origin`, `origin2`, `size`, `size2`, `length`,
  `cullrange`: game units, the world-space distance unit `origin` offsets and
  impact points already use elsewhere in the protocol (events doc section 2,
  `pos.trBase`). No independent unit label exists in the files. This is the
  only self-consistent reading, since these keys mix freely with `origin` in
  the same blocks; `fx/impacts/newimps/v_blast1.efx`-style explosion effects
  size their debris `radius`/`cullrange` in the same hundreds-to-thousands
  range as `origin` offsets.
- `rgb`, `alpha`: unit interval `[0, 1]`. Every observed value across the
  corpus falls in `[0, 1]`.
- `bounce`: unit-less restitution coefficient, values seen cluster in
  `[0.5, 2]`. A q3-style bounce can exceed 1.0 briefly through the physics
  integration, so this is not clamped to `[0,1]` the way `alpha` is.
- `density`, `variance`: game units, the spacing of an `Emitter`'s
  `emitfx` plays along its path and its jitter (R10). An earlier version
  of this list read them as a spawn rate and a timing jitter.
- `wind`, `nonUniformScale`: unit-less. `wind` is a multiplier on an external
  wind force, `nonUniformScale` is a boolean toggle (section 4).

## 6. Working set: effects the event mapping references

Cross-referenced against the events doc section 5. vcod only needs to parse
the `.efx` files reachable from the wire event table, not all 460.

### 6a. `fx/iw_impacts.csv` format

Read out of `pak5.pk3` alongside the `.efx` files (the same `unzip 'fx/*'`
pass catches it). Format, from the file's own header comments plus direct
inspection:

- 3 comma-separated columns, no header row: `impact_type,surface,effect_path`.
  Comment lines start with `#` and are quoted CSV (`"# text",,`). A real CSV
  parser or a simple `#`-stripping line splitter both work, since no data row
  contains a comment character.
- `impact_type` is one of 9 fixed literals: `bullet_small_normal`,
  `bullet_small_reflect`, `bullet_large_normal`, `bullet_large_reflect`,
  `grenade_bounce`, `grenade_explode`, `rocket_explode`,
  `molotov_explode_normal`, `molotov_explode_reflect`.
- `surface` is one of the 23 surface-type names from the events doc section 4
  (`default`, `bark`, ..., `asphalt`), lowercase, matching the numeric
  `surfType` index by name.
- `effect_path` may be blank. Both forms appear: `bullet_small_reflect,foliage`
  with no trailing comma at all, and `grenade_bounce,gravel,` with one. A
  blank means no effect plays for that (type, surface) pair.
- Rows can repeat for the same (type, surface) pair within the file.
  `bullet_small_normal,bark` appears twice, both times mapping to
  `fx/impacts/default_hit.efx`; harmless since they agree. This is not the
  override rule below, which is about later files, not later rows within one
  file.
- Override rule: stock ships exactly one CSV (`fx/iw_impacts.csv`), so the
  alphabetical override the events doc describes never triggers in retail. It
  only matters for mods and custom CSVs, out of scope for vcod parsing retail
  assets.
- Every row for `molotov_explode_normal`/`molotov_explode_reflect` is present
  and non-degenerate in the CSV, but per the events doc section 1,
  `EV_MOLOTOV_EXPLODE`/`_NOMARKS` have no case in the MP `CG_EntityEvent` jump
  table. These 50 rows (25 `_normal` + 25 `_reflect`, including the duplicate
  `bark`/`brick` rows every block has) are dead data for a multiplayer
  spectator, and vcod skips them.

### 6b. The working-set `.efx` list

32 distinct files, all present in the extracted corpus.

Bullet impacts (`bullet_small_normal`/`bullet_large_normal`; the `_reflect`
columns are blank for every surface in stock retail, so vcod never needs a
reflect-side effect):

```
fx/impacts/default_hit.efx        fx/impacts/small_grass.efx
fx/impacts/small_brick.efx        fx/impacts/small_gravel.efx
fx/impacts/large_brick.efx        fx/impacts/large_gravel.efx
fx/impacts/small_concrete.efx     fx/impacts/snowhit_small.efx
fx/impacts/large_plaster.efx      fx/impacts/snowhit_large.efx
fx/impacts/small_gravel2.efx      fx/impacts/metalhit_small.efx
fx/impacts/large_gravel2.efx      fx/impacts/metalhit_large.efx
fx/impacts/flesh_hit.efx          fx/impacts/small_rock.efx
fx/impacts/flesh_hit_noblood.efx  fx/impacts/large_rock.efx
fx/impacts/small_foliage.efx      fx/impacts/waterhit_small.efx
fx/impacts/small_glass.efx        fx/impacts/waterhit_large.efx
fx/impacts/large_glass.efx        fx/impacts/woodhit_small.efx
```

`flesh_hit_noblood.efx` is the one hardcoded literal path in the client,
swapped in for `surfType == 7` when the blood cvar is off (events doc section
4).

Grenade/rocket explosions (`grenade_explode`, `rocket_explode`;
`grenade_bounce` is blank for every surface, so no `.efx` is needed there):

```
fx/explosions/grenade1.efx
fx/explosions/grenade2.efx
fx/explosions/grenade3.efx
fx/explosions/grenade_water.efx
fx/impacts/snow_mortarlite.efx
fx/impacts/newimps/v_blast1.efx
fx/impacts/newimps/v_blast1dirt.efx
fx/impacts/newimps/v_blast1wood.efx
```

Not in the working set, deliberately excluded:

- All `molotov_explode_*` rows (unreachable in MP, section 6a).
- Muzzle flashes (`viewFlashEffect`/`worldFlashEffect`). These come from
  per-weapon files, not the impacts CSV, and the events doc does not enumerate
  specific weapon `.efx` paths, so there is no fixed list to cross-check.
  Whatever `.efx` files they name parse under the same grammar (they use the
  same 10 block types).
- Script/`ET_FX` effects selected via configstrings 780 to 843. These are
  per-map/per-mod data set at runtime, not statically enumerable from the
  retail asset set.

I read seven files in full and checked them line by line against the awk/grep
census output: `fx/error.efx` (230 bytes, minimal `Emitter`),
`fx/impacts/default_hit.efx` (3×`Particle` + `Decal`, working set),
`fx/impacts/n_waterimpact.efx` (`Tail`, `Line`, `Particle`, `FxRunner` in one
file), `fx/explosions/grenade2.efx` (`Particle`×3, `Emitter`, `Light`,
`Decal`, `FxRunner`, working set), `fx/explosions/shock.efx` (`Cylinder`),
`fx/atmosphere/rainrings.efx` (`OrientedParticle`), `fx/explosions/mutha2.efx`
(`FxRunner` + `Sound`). They span 9 of the 10 block types; no discrepancy.

---

# Renderer semantics (from CoDMP.exe / cgame_mp_x86.dll)

Everything above section 4 was reasoned from the corpus. This section replaces
that reasoning with the binary's own arithmetic for the keys that decide how
big things are drawn. Two corrections to where the code lives before the
findings:

- The `.efx` parser and the whole particle system live in `CoDMP.exe`, not in
  `cgame_mp_x86.dll`. The cgame only registers effect paths; every `.efx`
  keyword string (`cheapOrgCalc`, `orgOnSphere`, `nonUniformScale`, the
  block-type names) is in the engine binary. I decompiled `CoDMP.exe` from the
  1.1 install with Ghidra's `analyzeHeadless` (`-import CoDMP.exe -postScript
  ExportDecomp.java`, script in `tools/re/`). Addresses below without a module
  prefix are `CoDMP.exe` VAs.
- Bullet tracers are not an `.efx` effect at all, see R8.

## R1. Block types are an enum, and it has 13 members

`0x4931e0` maps the block keyword to an integer stored at template `+0x48`:

| id | keyword | id | keyword |
|---|---|---|---|
| 1 | `particle` | 8 | `orientedparticle` |
| 2 | `line` | 9 | `electricity` |
| 3 | `tail` | 0xa | `fxrunner` |
| 4 | `cylinder` | 0xb | `light` |
| 5 | `emitter` | 0xc | (unused, string @ `0x559dc0`) |
| 6 | `sound` | 0xd | (unused, string @ `0x559db8`) |
| 7 | `decal` | | |

Each template is `0x25c` bytes. The census's 10 observed keywords are a
subset; `electricity` parses but its renderer is a stub
(`"FXPORT RT_ELECTRICITY TDB\n"` at `0x5128a0`).

## R2. `size` is really `width`, `length` is really `height`

The curve-group dispatcher in the primitive key parser accepts these aliases:

| group | aliases | template offsets (start.a/start.b, end.a/end.b, parm.a/parm.b) | parser |
|---|---|---|---|
| `size` | `size`, `width` | `0x1fc`/`0x200`, `0x204`/`0x208`, `0x20c`/`0x210` | FUN_0049d380 @ `0x49d380` |
| `size2` | `size2`, `width2` | `0x214`/`0x218`, `0x21c`/`0x220`, `0x224`/`0x228` | FUN_0049d530 @ `0x49d530` |
| `length` | `length`, `height` | `0x22c`/`0x230`, `0x234`/`0x238`, `0x23c`/`0x240` | FUN_0049d6e0 @ `0x49d6e0` |

Each of `start`/`end`/`parm` is read with `sscanf("%f %f")` and a single value
fills both slots, so the 2-arity form is a min/max range sampled per particle
(`FUN_00431440` @ `0x431440` is the sampler), not a two-axis value. This
confirms census section 2b's reading.

## R3. The size/length curve evaluators, and what "no flags" means

Curve flag words are parsed by FUN_0049b3e0 @ `0x49b3e0` into a nibble:
`linear`=1, `random`=2, `nonlinear`=4, `wave`=8, `clamp`=0xc. Each group
shifts that nibble into its own slot of the template flag word at `+0xb8`
(alpha `<<0`, rotation `<<4`, size `<<8`, length `<<0xc`, size2 `<<0x10`).

Per frame, FUN_0048e3a0 @ `0x48e3a0` (size), FUN_0048f400 @ `0x48f400`
(length) and FUN_0048e4f0 @ `0x48e4f0` (size2) all run the same shape:

```
f = 1.0
if (linear)  f = 1 - (now - startTime) / (endTime - startTime)
switch (mode bits) { nonlinear / wave / clamp -> replace or average f }
if (random)  f *= rand01
value = f*start + (1 - f)*end
```

Without `linear` (and without a mode bit) `f` stays 1.0 and the curve holds
`start` for the entire life. An unflagged curve is constant, not an implicit
linear ramp. This is common: 101 of the corpus's 1089 `size` blocks carry no
`flags` line, and all 101 of them set `start`.

Evaluated results land on the primitive instance: current size at `+0xb8`,
current size2 at `+0xa0`, current length (tail only) at `+0x154`.

The corpus does carry `length` on non-tail blocks: the `muzflash2a`
`Particle` in `fx/muzzleflashes/heavy.efx` (`length 12 -> 275`, MP40, MP44,
BAR), `german_mg.efx`, `thompson.efx` and `russian_mg.efx` (`42 -> 375` and
`42 -> 275`). The sprite path (R4) reads only size and size2, so the value is
inert in retail. Inferred from the decompilation; honouring it as a tail
length drew a 60-140 unit streak trailing back through the shooter on every
MP40 shot, so vcod drops `length` on sprites.

## R4. `size` is a half-extent; a sprite draws `2 * size` across

The primitive instance embeds a `refEntity_t` at instance `+0x3c` (`0x9c`
bytes, copied wholesale by the add-entity call at `0x4e9a00`). Fields relative
to that base: `reType` `+0x00`, `origin` `+0x44`, `oldorigin` `+0x54`,
`shaderRGBA` `+0x6c`, current size2 `+0x64`, current size `+0x7c`, `rotation`
`+0x80`.

The backend dispatches on `reType` at FUN_005128a0 @ `0x5128a0` (4 = sprite,
0xc = oriented sprite, 0xd = tail/line, 0xf = cylinder).

Sprite path, FUN_005101d0 @ `0x5101d0`:

```
w = ent[0x7c]           // current size
h = ent[0x64]           // current size2
left = viewaxis[1] * w
up   = viewaxis[2] * h  // both rotated by ent[0x80] when it is non-zero
FUN_0050fe20(&ent[0x44], left, up, &ent[0x6c], 0, 0, 1.0, 1.0)
```

and the quad emitter FUN_0050fe20 @ `0x50fe20` writes the four corners as

```
v0 = origin + left + up
v1 = origin - left + up
v2 = origin - left - up
v3 = origin + left - up
```

So `size` is a radius-like half-extent and the drawn sprite spans `2 * size`
units. `size2` is the vertical half-extent; when `nonUniformScale` is off the
particle constructor FUN_0049f360 @ `0x49f360` copies the `size` curve into
the `size2` slots, so the sprite is square.

The `Cylinder` primitive (FUN_00511350 @ `0x511350`) uses the same two values
as ring radii: `size2` at `origin`, `size` at `oldorigin`.

## R5. A `Tail` is anchored at its head, and `length` is the full length

The per-frame tail update at `0x48f360` stashes the pre-move origin into
instance `+0x13c`, moves the particle, then calls the geometry step
FUN_0048f550 @ `0x48f550`:

```
delta = prevOrigin(+0x13c) - origin(+4)
VectorNormalize(delta)                    // 0x42db90
oldorigin(+0x90) = origin + delta * length(+0x154)
```

The renderer spans that pair verbatim. FUN_00511250 @ `0x511250` normalizes
the screen-space side vector and calls FUN_00511010 @ `0x511010`, whose four
verts are

```
v0 = A + w*side   (st 0,0)      v2 = B + w*side   (st 0,1)
v1 = A - w*side   (st 1,0)      v3 = B - w*side   (st 1,1)
```

with `w = ent[0x7c]` (the current `size`). Therefore:

- the segment runs from the particle (the head) backwards along its direction
  of travel; it is not centred on the particle;
- its total extent is exactly `length`, not `2 * length`;
- its total width is `2 * size`, consistent with the sprite path.

## R6. Spawn distribution: `cheapOrgCalc`, `orgOnSphere`/`orgOnCylinder`

From the spawner FUN_00494050 @ `0x494050` (template `origin` ranges at
`+0xdc`..`+0xf0`, `height` at `+0x10c`/`+0x110`, `radius` at
`+0x114`/`+0x118`, `spawnFlags` at `+0xbc`):

- Normally the sampled `origin` is rotated into the emitter's axis frame
  (`origin.x*axis0 + origin.y*axis1 + origin.z*axis2`).
- `cheapOrgCalc` (0x100) skips that rotation and uses the sampled `origin` as
  raw world-space offsets. A cost optimisation, not a distribution change.
- `orgOnSphere` (0x1) places the point on a sphere from `radius`;
  `orgOnCylinder` (0x4) from `radius` + `height`. `radius`/`height` are read
  only under these two flags. A `Tail` that sets `radius`/`height` without
  either flag (e.g. `fx/tagged/tracers.efx`) ignores both.
- `absoluteVel` (0x400) / `absoluteAccel` (0x800) keep the sampled
  velocity/acceleration in world space instead of the emitter frame.
- `evenDistribution` (0x2000) is tested exactly once, inside the
  `Emitter`-kind (model / chained-fx) spawn path. It does not affect the
  geometry of `Particle`/`Tail` primitives.
- Remaining bits, for completeness: `org2fromTrace` 0x10, `traceImpactFx`
  0x20, `org2isOffset` 0x40, `cheapOrg2Calc` 0x200, `axisFromSphere` 0x2,
  `randrotaroundfwd` 0x1000, `rgbComponentInterpolation` 0x4000,
  `affectedByWind` 0x10000, `lessAttenuation` 0x20000.

No per-effect or global size scale factor exists anywhere on these paths, and
muzzle-flash world effects get no special clamping. They are ordinary
`Particle`/`Tail`/`Light` primitives.

## R7. Every "missing key" default, from the template initializer

Templates are `memset(t, 0, 0x25c)` by the allocator FUN_00497640 @
`0x497640`, then handed to one default initializer, FUN_0049a460 @
`0x49a460` (called at `0x49337a`, before the key parser runs). That
initializer writes exactly four non-zero groups, all `1.0f`:

| template offsets | key | default |
|---|---|---|
| `+0x54`/`+0x58` | `count` | 1 (confirms section 4's reasoned value) |
| `+0x5c`/`+0x60` | `life` (see the note below) | 1 |
| `+0x10c`/`+0x110` | `height` | 1 |
| `+0x114`/`+0x118` | `radius` | 1 |
| `+0x1ac`..`+0x1d8` | `rgb` start+end (2 x 3 x 2) | white |
| `+0x1e4`..`+0x1f0` | `alpha` start+end | 1.0 |

Correction, 2026-10-12. VERIFIED, CoDMP.exe 1.1: the `life` handler
0x49b5f0 stores its pair at `+0x5c`/`+0x60`, and the `bounce`/`intensity`
handler 0x49b710 stores at `+0x254`/`+0x258` (and ORs 0x2000000 into
`+0xb8`). So the initializer's first pair is `life`, and an absent `life`
reads as 1 ms, not 0; this table used to name that pair `bounce`. The
`Decal` spawn passes `ftol(life)` as the mark's lifetime (0x496941..0x49696f,
`cod11-events-and-fx.md` section 9). vcod reads an absent `life` as 1 ms
too (`fx::efx::Emitter::default`): a particle lives until its age passes
1 ms, a decal's mark expires the first frame more than 1 ms later.

Everything else stays 0: `delay`, `origin`, `velocity`,
`acceleration`, `gravity`, `rotation`, `rotationDelta`, `cullrange`, and the
whole `size`/`size2`/`length` curve block (`+0x1fc`..`+0x240`).

The practical consequence is that `start` and `end` share one default per
curve, so a curve that supplies only one of them still interpolates against
the other: `size { start 210 flags linear }` fades 210 -> 0, and
`alpha { start 0.5 flags linear }` ramps 0.5 -> 1.0. Section 4 carries the
per-key rows.

`cullrange 0` (the default) reads as "never culled" on this path: the
spawner's `if (cullrange == 0)` branch skips the distance check entirely.

## R8. Bullet tracers are hardcoded, not an `.efx`

`fx/tagged/tracers.efx` is the anti-aircraft gun tracer effect (velocity
~11 900 u/s, 3.7 to 4.3 s life, a `Tail` 120 units long and 10 to 20 units
wide with the `gfx/effects/antiaircraft_tracer` shader). It is not what a
rifle bullet draws.

`CG_Tracer` @ `cgame_mp_x86.dll 0x30039590`, called from `CG_BulletHitWall`
(173/174) and `CG_BulletHitFlesh` (175/176, always `surfType` 7; events doc
section 2), rolls `cg_tracerchance` and
hands the muzzle/impact pair to one of two hardcoded paths, picked by the
surface type argument: VERIFIED, `cmp [esp+0x20],7` at `0x300395fb` sends
`surfType == 7` (flesh) to the segment setup at `0x30039340` with a pushed 0,
and every other surface to `CG_SpawnTracer` @ `0x30038f30`.

**Every surface but flesh: a moving streak.** `CG_SpawnTracer` takes the
impact in `eax` and the muzzle in `ecx` (VERIFIED, `0x30039604` and
`0x30039600`) and allocates a local entity (`CG_AllocLocalEntity` @
`0x3001ff50`) whose fields it writes as follows, all VERIFIED from
`0x30038f30`..`0x300390af`:

```
dir       = VectorNormalize(impact - muzzle)          // 0x30039cc0
travel    = |impact - cg_tracerlength*dir - muzzle|   // sqrt @ 0x3005ac90
le+0x08   = 2                                         // leType
le+0x18   = 2                                         // pos.trType, TR_LINEAR
le+0x1c   = cg.time - (rand() % cg.frametime) / 2     // pos.trTime, integer ms
le+0x24   = muzzle                                    // pos.trBase
le+0x30   = cg_tracerSpeed * dir                      // pos.trDelta
le+0x10   = trTime - ftol(travel * -1000 / cg_tracerSpeed)   // endTime
```

The field names in the comments are INFERRED, from how `0x3001ffe0`,
`CG_AddLocalEntities` and `BG_EvaluateTrajectory` read those offsets. The
`-1000.0` is `ds:0x30069594`. VERIFIED: `ftol` @ `0x3005a890` is a
`fistp` with a truncation fix-up, so the lifetime is `travel / speed` in whole
ms, truncated. The two globals read at `0x30039029` and `0x3003904d`
(`0x30207144`, `0x30207148`) are named `cg.frametime` and `cg.time` here
INFERRED from use: the second is what `CG_AddLocalEntities` compares
`endTime` against.

`CG_AddLocalEntities` @ `0x300201b0` frees an entity once `0x30207148 >=
le+0x10` (VERIFIED, `cmp`/`jl` at `0x300201c9`) and dispatches `leType` 2 to
`0x3001ffe0` (VERIFIED, `0x3002021a`), which evaluates the trajectory at
`cg.time` through `BG_EvaluateTrajectory` @ `0x30005470` (VERIFIED: its jump
table at `0x3000575c` sends type 2 to `0x300054a0`, `trBase + trDelta *
(time - trTime) * 0.001`, the `0.001` at `ds:0x300693c0`), normalizes
`trDelta` and draws `CG_DrawTracer(p, p + cg_tracerlength * dir)` (VERIFIED,
`0x30020007`..`0x30020045`). So the trajectory point is the tail:

```
p(t) = muzzle + cg_tracerSpeed * dir * (t - trTime)
draw [p(t), p(t) + cg_tracerlength * dir] while t < endTime
```

The tail starts on the muzzle, or ahead of it by the half-frame back-date,
and the head reaches the impact on the last frame. There is no minimum
distance on this path (VERIFIED: `CG_SpawnTracer` reads no constant but
`-1000.0`), and a shot shorter than `cg_tracerlength` draws its whole streak
past the impact for `(length - dist) / speed`. United Offensive's spawn adds that gate, and
widens tracers under `sv_night` (coduo-diff.md 19, "The tracer cvars").

**Flesh: a one-frame segment.** The setup at `0x30039340` (VERIFIED,
`0x30039343`..`0x30039426`):

```
len = VectorNormalize(end - start)
if (len < 100.0 && arg == 0)            return          // ds:0x30069524
frac    = 50.0 + (len - 60.0) * rand() * (1/32768)     // ds:0x3006958c / 0x30069590 / 0x300693b4
endDist = min(frac + cg_tracerlength, len)             // ds:0x302999e8
CG_DrawTracer(start + frac*dir, start + endDist*dir)
```

It allocates nothing and only adds the quad to the scene, so it is drawn on
the frame of the event and never again. INFERRED: the syscall `0x40` at the
end of `CG_DrawTracer` adds a polygon to this frame's scene; it is the same
call the other one-shot quads go through.

CG_DrawTracer @ `0x300390c0` emits one quad whose corners are
`A ± cg_tracerwidth*side` / `B ± cg_tracerwidth*side`, `side` being the
normalized screen-space perpendicular to the segment, with the
`gfx/misc/tracer` shader (registered in CG_RegisterGraphics @ `0x30020da0`).
VERIFIED, from the vertex writes: `u` runs along the segment, 1 at `B` (the
head) and 0 at `A` (the tail).

Cvar defaults, read out of the cgame's cvar table at `0x30074e54`..`0x30074e8c`:

| cvar | struct VA | value VA | default |
|---|---|---|---|
| `cg_tracerchance` | `0x301df440` | `0x301df448` | `0.4` |
| `cg_tracerwidth` | `0x3020d820` | `0x3020d828` | `0.8` (a half-width, 1.6 units across) |
| `cg_tracerSpeed` | `0x301de900` | `0x301de908` | `4500` |
| `cg_tracerlength` | `0x302999e0` | `0x302999e8` | `160` (the full streak length) |

This also corrects the events doc's naming: `0x30039340` is the flesh tracer's
segment setup, not `CG_BloodSpray`. An earlier version of this section read
that routine as the setup for every tracer; vcod then anchored a moving
streak's head 50 units out, which put its tail 110 units behind the muzzle
for its first two frames.

## R9. Physics, `bounce`, and the `Emitter` debris model

The effect system is a class hierarchy with a vtable per primitive; the
`Emitter` instance is `0x18c` bytes, built by `0x4a02c0` (vtable
`0x559f48`). Instance offsets below are that object's; `frametime` is the
engine's frame time in ms at `0x1407514`.

- VERIFIED, the flags parser (`0x49bb30`, the `useModel` / `usePhysics`
  / `impactKills` / `impactFx` string compares): `useModel` 0x1000000,
  `useBBox` 0x4000000, `usePhysics` 0x2000000, `impactKills` 0x40000000,
  `impactFx` 0x80000000, `deathFx` 0x20000000, `emitFx` 0x10000000,
  `useAlpha` 0x8000000, `relative` 0x200000. `expensivePhysics` is
  accepted and sets nothing.
- VERIFIED: the `bounce` handler `0x49b710` ORs 0x2000000 (`usePhysics`),
  the `impactfx` list parser `0x49cbd0` ORs 0x82000000 (`usePhysics` and
  `impactFx`), the `models` list parser `0x49cae0` ORs 0x1000000
  (`useModel`). So `bounce` alone makes a particle physical.
- VERIFIED: an `impactfx` entry goes through `0x492eb0`, which copies the
  name up to its first `.` and lowercases it. INFERRED: the list names an
  effect without its extension (`fx/shellejects/shelleject_pistolsound`)
  and `.efx` is put back on.
- VERIFIED, the spawner `0x494050`: `gravity` (template `+0x194`/`+0x198`,
  sampled) is added to the acceleration's z.
- VERIFIED, the `Emitter` arm (case 5) of the same spawner samples three
  `angle` pairs (`+0x134`..`+0x148`) and adds them to the angles of the
  spawn axis (`0x42e470`), samples three `angleDelta` pairs
  (`+0x14c`..`+0x160`), and samples `bounce` (`+0x254`/`+0x258`).
  VERIFIED, `0x4a02c0`: the instance stores angles at `+0x168`, angle
  deltas at `+0x174` and its elasticity at `+0x138`. INFERRED: the sampled
  `bounce` is the elasticity, the argument order being the spawner's.
- VERIFIED, `CParticle::UpdateOrigin` (my name, `0x48df20`): first
  `vel += accel * frametime * 0.001` (`0x48e340`), then
  `new = org + vel*t + 0.5*accel*t*t` on the new velocity. With
  `usePhysics` it traces `org` to `new` through `0x424530`, model 0,
  brush mask 1 (`CONTENTS_SOLID`), a point unless `useBBox` passes the
  instance's box (`0x48e0e2`..`0x48e123`). On a hit with neither
  start- nor all-solid: an `impactFx` particle plays its impact effect at
  the trace's hit; an `impactKills` particle dies; otherwise
  `vel += accel * fraction * t`, `vel -= 2 * (vel . n) * n`,
  `vel *= elasticity`, and when `n.z > 0` and the new `vel.z < 4`
  (constant at `0x569080`) velocity and acceleration are zeroed and
  `usePhysics` and `impactFx` cleared (`& 0x7dffffff`). The origin moves
  to `org + (new - org) * fraction`. Without a hit it moves to `new`.
  INFERRED, from the call sites at `0x48dea5`, `0x48ed15`, `0x48f394`,
  `0x48fa1c` and `0x491272`: every moving primitive integrates this way,
  sprites included.
- VERIFIED, `CEmitter::Update` (my name, `0x48f9e0`): when
  `UpdateOrigin` leaves the origin where it was, the angle deltas are
  multiplied by 0.7 (`0x5691a0`); then `0x490300` adds
  `frametime * 0.01 * angleDelta` to the angles (`0x568f00`) and turns
  them into the model's axes. With `useModel` the model's axes are scaled
  by the instance's size (`+0xb8`) and the refEntity goes to the scene
  (`0x48f9c0`). INFERRED: `angleDelta` is degrees per 100 ms, a resting
  model's spin dies away by 30% a frame, and the model is drawn with no
  fade until `life` runs out.
- VERIFIED, the effect runner `0x493870`: each element reads its
  `cullrange` (`+0x64`) before its spawn count is drawn. INFERRED, with
  the decal note in the events doc (section 9): an element is skipped at
  spawn when it is past its `cullrange` from the view.

vcod: `fx::sim::update_origin` and `step_pool` follow the above for every
particle; an `Emitter` is a particle with `Debris` (model, angles), drawn
through `FxSystem::models`. Every element's `cullrange` is tested once at
spawn against `FxSystem::set_view`'s camera. Chaining, `relative` and
`useBBox` are R10.

## R10. Chaining (`playfx`, `emitfx`, `impactfx`, `deathfx`), `relative`, `useBBox`

Addresses are `CoDMP.exe` 1.1 VAs; offsets on a template are the `0x25c`
template's (R1), offsets on an instance the primitive's. I decompiled the
functions not in the full export with `tools/re/DecompAt.java`.

### The lists

- VERIFIED, the primitive key parser `0x49d8b0`: `impactfx` goes to `0x49cbd0`, `deathfx` to
  `0x49cce0`, `emitfx` to `0x49ce00`, `playfx` to `0x49cf20`. Each
  registers every entry through `0x492eb0` into a vector: impact at
  template `+0x78`, death at `+0x88`, emit at `+0x98`, play at `+0xa8`.
  `0x49cce0` ORs `0x20000000` (`deathFx`) into `+0xb8`, `0x49ce00` ORs
  `0x10000000` (`emitFx`), `0x49cbd0` ORs `0x82000000` (R9); `0x49cf20`
  sets no flag. Their error strings name them:
  `CPrimitiveTemplate::ParseDeathFxStrings` / `ParseEmitterFxStrings` /
  `ParsePlayFxStrings`.
- VERIFIED, `0x492780`: a list is drawn from with the engine's LCG
  (`* 0x343fd + 0x269ec3`, state at `0x57b4b0`), one entry per spawned
  primitive; an empty list yields effect id 0.
- VERIFIED, `density` (`0x49c230`) stores its pair at template
  `+0x19c`/`+0x1a0`, `variance` (`0x49c280`) at `+0x1a4`/`+0x1a8`.
- VERIFIED, the paks (`tools/efx_chains.py`, 1.1 install, 459 files): 109
  `emitfx` entries in 91 files, all in `Emitter` blocks; 116 `playfx` in
  89 files, all in `FxRunner`; 19 `impactfx` in 11 files (`Emitter`,
  `Tail`); 16 `deathfx` in 12 files (`Particle`, `OrientedParticle`,
  `Tail`, `Line`). Every target exists. The deepest chain is 5 effects
  (`fx/explosions/v2_exlosion.efx`); `fx/explosions/moltov_hit.efx` names
  itself in its `impactfx`. No block sets a chaining flag without its list
  or the reverse.

### `FxRunner`

- VERIFIED, the effect runner `0x493870` (`CFxScheduler::PlayEffect` by its
  error string): for each element it tests `cullrange` (R9), draws `count`,
  and per instance draws `delay` and truncates it (`0x538be0`). Under 1 ms
  it calls the spawner `0x494050` at once; otherwise it queues a `0x44`-byte
  record (effect id, element index, due time `0x140750c + delay`, origin,
  axis, bolt) that `0x493f40` hands to the spawner once due, with the
  overshoot as its last argument.
- VERIFIED, the spawner's case 10 (`0x494050`, template type `0xa`):
  `PlayEffect(pick(+0xa8), origin, axis, bolt)`, the origin and axis the
  spawner computed for the element (`origin` rotated unless
  `cheapOrgCalc`, then `orgOnSphere`/`orgOnCylinder`, R6). INFERRED: the
  child effect runs its own runner loop inside the call, so an undelayed
  `FxRunner` chain plays within one `PlayEffect`.
- VERIFIED: nothing on this path counts depth. INFERRED: a self-referencing
  `playfx` would recurse without end; the stock one self-reference is an
  `impactfx`, which only replays on a later frame's hit.

### `emitfx`

- VERIFIED, the spawner's case 5 hands the `Emitter` constructor `0x4a02c0`
  `pick(+0x88)`, `pick(+0x78)`, `pick(+0x98)` and the sampled `density`
  and `variance` (pushed at `0x4967fe`/`0x4967ff` from the slots
  `0x496620` and `0x4965ed` filled). The constructor stores them at
  instance `+0x38` (death), `+0x34` (impact), `+0x180` (emit), `+0x184`
  (density), `+0x188` (variance), the spawn time at `+0x160`, and
  `+0x164 = (2 * rand() * (1/32768) - 1) * variance + density`.
  `0x49e690` and `0x49e6d0` copy its origin and velocity to `+0x13c` and
  `+0x148`.
- VERIFIED, `CEmitter::Update` (`0x48f9e0`) after the move, with
  `emitFx` set (`0x48fcd4`): from `clock = +0x160` and `s = 0`, while
  `clock < now`: `s += 12` (`0x48fd10`); `p = +0x13c + +0x148 * s +
  0.5 * accel * s^2` (seconds, `accel` the instance's current `+0xe8`);
  when `|p - +0x13c|^2 >= +0x164^2` it takes one Newton step on that
  squared distance (`3` at `0x5690c8`, `0.25` at `0x568ed0`, `1000` at
  `0x569090`), floors the result at the previous step, calls
  `PlayEffect(+0x180, p(s), the instance axis +0x58, bolt)`, moves
  `+0x13c`/`+0x148` to `p(s)` and its velocity, adds `s` to `+0x160` and
  draws a new spacing as above (`0x4902a9`); otherwise `clock += 12`.
  The only other writes to `+0x13c`/`+0x148` are the save/load pair at
  `0x4903d0`.
- INFERRED from those writes: the plays follow the emitter's launch arc,
  not its traced path, so a bounce does not move them, and once the
  emitter rests (R9 zeroes its acceleration) they carry on in a straight
  line at the launch-arc velocity. The walk samples one 12 ms step past
  `now`, so a play can land up to 12 ms ahead of the frame.
- INFERRED, from that loop: `density` is a distance, not a rate, and
  `variance` its jitter. A
  `variance` above `density` (`grenade2.efx`'s particulates, `density 4
  variance 14`) can draw a spacing near zero, and the Newton step then
  lands about half a step after the last play.

### `impactfx` and `deathfx`

- `impactfx` plays where a physics particle hits (R9), through `0x493730`
  with no bolt.
- VERIFIED, `Die` (`0x48dda0`, slot 1 of every primitive vtable, e.g.
  `0x559f48`/`0x559f80`): with `deathFx` set and `impactKills`
  (`0x40000000`) clear it builds a direction from three `rand() / 32768 *
  2 - 1` draws, normalizes it (`0x42db90`) and plays `+0x38` at the
  instance origin `+4` through `0x493730`, with no bolt.
- VERIFIED, the pool walk `0x49f1e0` (1800 slots, `0x708`): a primitive
  past its end time has `0x40000000` cleared (`& 0xbfffffff`) before
  `Die`; one whose `Update` (slot 2) returns false gets `Die` with the bit
  as it was. INFERRED: an expired element always plays its `deathfx`, an
  `impactKills` hit never does, and any other failed update (a lost bolt)
  does.
- VERIFIED, `0x49ecb0`: a full pool shows `gfx/2d/warning@maxeffects.jpg`
  (behind a cvar) and recycles slot 0 through `Die`.

### `relative`

- VERIFIED, the flags parser `0x49bb30`: `relative` ORs `0x200000`.
- VERIFIED, the spawner (`0x494050`, the test at `0x49547c`): with
  `0x200000` set and a bolt whose entity number is not
  negative, it takes a shared bolt record (`0x48d6c0`, a list at
  `0x1407320` keyed on entity and bone, reference counted, freed by
  `0x48d730`) and stores it at instance `+0xd8`; when the bolt cannot be
  posed (`0x48d770` returns 0) it creates nothing (the jump to `0x497080`
  at `0x4954d8`).
- VERIFIED, `0x49e8d0`: a bolt is posed from the entity's origin and axis
  (`0x460480` with `0xc`) and, for a bone index of 0 or more, that bone's
  matrix (`0x460480` with `0xb`); `0x48d770` re-poses it when its stamp
  differs from the counter at `0x163a218`. INFERRED: that counter is the
  frame number, so a bolt is posed once a frame.
- VERIFIED, the `Emitter` constructor's bolted branch: it moves the world
  origin, velocity and acceleration into the bolt's frame (`0x44b890`,
  `0x44b8e0`). `UpdateOrigin` (`0x48df20`) and the draw (`0x44b7f0`)
  transform back through the bolt each frame; a bolt that fails to pose
  has its record marked dead and the primitive's update fails.
- INFERRED, from the callers: only effects played on a tag pass a bolt
  (syscall `0xe5`, `cod11-events-and-fx.md` 5b, 5d), so `relative` matters
  only for muzzle flashes and brass. The stock users are 26 muzzle flashes
  and 11 tagged or map effects (`tools/efx_chains.py -v`).

### `useBBox`

- VERIFIED: the `min` / `max` keys (`0x49b550` / `0x49b5a0`, key strings at
  `0x55962c` / `0x559628`) store a box at template `+0xc4`/`+0xd0` and OR
  `0x6000000` (`useBBox` and `usePhysics`); the flag alone is `0x4000000`
  (`0x49bc80`). `UpdateOrigin` traces that box instead of a point when the
  bit is set (`0x48e0e2`).
- VERIFIED, the paks: no stock block sets `useBBox`, `min` or `max`.

### vcod

`crates/client/src/fx/sim.rs`:

- `FxRunner` is `Behavior::Runner`: per instance, its origin and spawn
  frame, then a `Chain` due after its `delay`. `FxSystem::spawn_marked`
  plays the undelayed ones before it returns; `FxSystem::spawn_chained`,
  called each frame after `step`, plays the rest along with every
  `impactfx`, `emitfx` and `deathfx` the step raised.
- An `Emitter` with `emitfx` carries the `Emit` cursor and walks it as
  above, in integer ms. The Newton step is floored at 1 ms past the last
  play as well, so a zero spacing cannot spin; retail has no such floor.
  The child plays along the emitter's forward, not its full axis, and
  without the `useModel` size scale on that axis.
- `deathfx` plays on expiry and on a lost tag, never on an `impactKills`
  hit.
- Chains stop at `MAX_CHAIN_DEPTH` (8); retail's only limit is its pool.
- `relative`: `Resolved::Spawn::bolt` names the shooter's `ctx.muzzles`
  key for a muzzle flash, `FxSystem::set_bolts` takes the muzzles each
  frame, and a relative element keeps its origin, velocity and
  acceleration in the frame `tangent_basis` builds on the muzzle's
  forward (the frame a `Directed` spawn uses), so its roll is not the
  tag's. It dies when its key leaves the map. A bolted particle does not
  trace. Brass passes no bolt; its two stock effects have no `relative`
  element. An `EV_PLAY_FX_ON_TAG` effect is bolted to its tag's full
  axis instead (`FxSystem::spawn_on_tag`, `set_tag_bolts`,
  `cod11-events-and-fx.md` 2, "playFxOnTag").
- A delayed element on a bolt spawns at the pose of the play's frame;
  retail poses the bolt again when it falls due (0x493f40).
- `useBBox` is not implemented; nothing in the paks reaches it.
- `cullrange` is tested once, at spawn, for every element, sprites and
  decals included (R9).

## R11. The axis an effect plays on: syscalls `0xe3` and `0xe4`

cgame plays a script effect through one of three syscalls, dispatched at
CoDMP.exe 0x4040b8..0x4040e0 (the `case 0xe3`/`0xe4`/`0xe5` arms of the
cgame syscall switch). Each builds a 3x3 axis and calls `PlayEffect`
(0x493870); the axis is the frame an element's `origin`, `velocity` and
`accel` are written in (R6), forward first.

- VERIFIED, `0xe3` (0x4936c0, position only): the axis is the constant
  `(0,0,1), (1,0,0), (0,1,0)` built on the stack before the call, so an
  effect played with no direction points up, its right is world +x and
  its up world +y.
- VERIFIED, `0xe4` (0x4a1800, position and direction): the direction is
  copied to axis[0] (`[esp+8]`), then 0x42e1b0 is called with `edi` on
  axis[0], `eax` on axis[1] (`[esp+0x14]`) and `ebx` on axis[2]
  (`[esp+0x20]`). 0x42e1b0 is Q3's `MakeNormalVectors`: right =
  `(f.z, -f.x, f.y)` less its projection on `f`, normalized (0x42db90),
  and up = right x forward.
- `0xe5` plays on a tag's own axis (R10, "relative"): it hands
  `PlayEffect` a null axis and the bolt, whose pose replaces the origin
  and axis (`cod11-events-and-fx.md` 2, "playFxOnTag"). vcod:
  `SpawnAt::Oriented`.

`EV_PLAY_FX` and an `ET_FX` entity with a zero `origin2` take `0xe3`;
`EV_PLAY_FX_DIR`, an `ET_FX` with a forward, and every impact and
explosion event take `0xe4` (`cod11-events-and-fx.md` 2). vcod:
`fx::sim::tangent_basis` is `MakeNormalVectors` and `SpawnAt::Point`
plays on `UP_AXIS`; before this the tangent basis was vcod's own choice,
which turned every directed effect's sideways spread by an arbitrary
angle about its forward, and a `Point` spawn had world axes (forward +x).

## What vcod implements from this, and what it doesn't

Implemented in `crates/client/src/fx/sim.rs`: R3 (unflagged curves hold
`start`; `length` only on `Tail`/`Line`), R4's half-extent reading, R5 (tail anchoring and full length), R6's
spawn-flag handling, R7's per-key defaults, R8 (`FxSystem::spawn_tracer`,
fed by `Resolved::Tracer` from 173 to 176), and R9 (physics, `bounce`,
`impactfx`, `Emitter` debris models), R10 (chaining, `relative`) and R11
(the play axes).

Not implemented, deliberately:

- `size2` / `nonUniformScale`. The `Emitter` struct in
  `crates/client/src/fx/efx.rs` has no `size2` field, so every sprite is
  square. 168 of the 1089 size-carrying blocks set `size2`; none of the
  muzzle-flash or impact effects vcod plays today do.
- The `nonlinear` / `wave` / `clamp` / `random` curve modes. Only the
  `linear`-vs-constant distinction is honoured. One `size` block and five
  `alpha`/`rgb` blocks in the whole corpus use a mode other than `linear`.
- `electricity`, `useBBox`, `evenDistribution`.
