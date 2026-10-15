# wowdecomp (WoW 3.3.5a decompilation) — research and feasibility for Dungeon Sandbox

Date: 2026-10-15. Scope: research only. Nothing from either external repository
was copied, vendored, translated or added as a dependency. Both were
shallow-cloned read-only into a scratch folder outside this source tree and
inspected there. This is an engineering assessment, not legal advice.

Repositories examined:

| | wowdecomp | Benilla |
|---|---|---|
| URL | https://github.com/SatyPardus/wowdecomp | https://github.com/samwhosung/benilla |
| Commit read | `0e4e9fe` (2026-10-15) | `8c35874` (2026-10-15) |
| Size | 3,555 tracked files | 1,929 tracked files |

Evidence labels used throughout:
* **[verified]**: checked in the repository (path given) or with the GitHub
  API.
* **[README]**: a claim the project makes that I could not check.
* **[inference]**: my reading, with the reason given.

---

## 1. Executive summary

**wowdecomp** is a source reconstruction of Blizzard's proprietary `Wow.exe`
3.3.5a (build 12340).
* **Structure**: every function carries the address it was recovered from, and
  the tree follows Blizzard's original module and file names.
* **Scale**: 22,597 `// OFFSET:` tags [verified], and 90.3% of the binary's
  functions by its own measure [README].
* **Requirements**: it needs a retail 3.3.5a client's `Data` folder and
  connects to a game server.
* **License**: none. There is no license file and GitHub reports
  `license: null` [verified].

The code is a derivative of Blizzard's copyrighted executable. Nothing in it
grants reuse rights.

**For Dungeon Sandbox it is not a usable source of code or assets.** Nearly
everything that defines our game lives on the server in WoW, not in this
client:
* combat, threat, enemy AI, pathfinding and encounter scripting;
* what this client holds instead is a mirror of server messages, plus
  rendering, UI plumbing, sound and networking [verified: §5].

The client-side systems that interest us are:
* indoor rendering with portals;
* model animation and attachments;
* the third-person camera;
* movement collision;
* spell visuals.

Each of these rests on general techniques that are documented in the
open graphics and game-programming literature, and mostly supported directly
by Bevy 0.20.

**Benilla** is a different kind of project. It is an independently written
Rust/Bevy 1.12.1 client under MIT OR Apache-2.0, built against a separately
published reverse-engineering reference.
* Its license is clear.
* It targets Bevy 0.18.1, and we pin 0.20.
* Its loaders read only Blizzard formats, which we must not use.

The parts of it that matter to us are architectural, and our 0.6 review
(ASSET_PIPELINE_RESEARCH.md) already took them.

**Recommendation in one line:** use nothing from wowdecomp. Let it, together
with Benilla's module docs and the standard literature, confirm a short list of
*general techniques* worth building independently. The most valuable are:
1. modular room prefabs with separate visual and walkable geometry;
2. room-and-doorway visibility computed from Dungeon Forge's plan;
3. per-room lighting and fog;
4. animated GLB characters with attachment points;
5. a data-driven ability-visual layer;
6. a standalone dungeon viewer.

See §12 for the full recommendation and its conditions.

---

## 2. Repository status and provenance

### 2.1 What has been reconstructed [verified unless marked]

**Build**: CMake, Windows only (`CMakeLists.txt` refuses other systems), C++17,
Visual Studio 2022. x64 is the main target; Win32 is kept "to check struct
layouts against the 32-bit original" (`CMakeLists.txt`, README).

**Layout**: `Engine/Source/<Module>` and `WoW/Source/<Module>` mirror the
original client's module and file names (README "Layout"). Engine modules
listed in `CMakeLists.txt`:

| Area | Engine modules |
|---|---|
| Core and platform | Storm, BlizzardCore, Base, Os, Event, Services, Tempest, Scrn |
| Graphics | Gx (Direct3D 9, D3D9Ex, OpenGL), Gxu, Model2 (M2 models), ShaderEffect, FFX (full-screen effects) |
| UI | Frame, FrameScript, FrameXML |
| Sound | SoundEngine (with FMOD Ex) |
| Other | lua-5.1, jpeg, pcre, BLPFile, Images, ComSat, SteelseriesMouse |

Game modules: WorldClient (world and map rendering), Object (units, players,
movement, game objects), Magic (spells, spell visuals), LiquidSystem, Ui,
UIUtil (camera, UI helpers), DB (client tables), Net, Battlenet, Glue
(login screens), WowServices and more.

**Size by module** (lines of `.cpp/.h/.c`, counted locally):

| Module | Lines |
|---|---|
| `WoW/Source/Ui` | 149k |
| `WoW/Source/Object` | 88k |
| `WoW/Source/WorldClient` | 47k |
| `Engine/Source/Frame` | 38k |
| `Engine/Source/Gx` | 23k |
| `Engine/Source/Model2` | 16k |
| `WoW/Source/Magic` | 12k |
| `WoW/Source/LiquidSystem` | 2.7k |
| `Engine/Source/SoundEngine` (mostly `fmodex/`) | 270k |

**History**: 232 commits, all by one author, from 2026-09-23 ("Initial
commit"). The GitHub repository was created on 2026-10-02.
* [inference] Reaching 90% of a 5.6 MB binary in about three weeks of public
  history suggests heavy tool assistance or earlier private work. Either way
  the history doesn't document provenance beyond "from the binary".

**Coverage** [README]: 24,515 of 27,157 functions (90.3%), 93.2% of code bytes,
measured on 2026-10-15. "A function counts as ported when its address is
tagged." The README itself says the share measures correspondence, not
correctness.

### 2.2 How complete, and does it work?

The README says the client "builds into a complete executable and the game is
100% playable" and warns that "a ported function can behave differently from
the original". **Not verified here.** Testing it would mean:
* building with MSVC;
* supplying a retail 3.3.5a `Data` folder;
* connecting to a game server.

None of that serves this assignment, and the second and third steps involve
proprietary data and third-party servers. Treat "fully playable" as an
unverified claim.

Some files are stubs. For example, `WoW/Source/Object/Collide.cpp` is one line
and `Collide.h` four [verified]. The functionality those names suggest lives
in `Movement.cpp` (`CMovement_C::BuildCollisionVolumePlanes`,
`IsFacetWall`, `IsFacetOverhead`, `GetStepUpHeight`,
`ComputeSteepSurfacePushNormalImpl` [verified function names]).

### 2.3 Correspondence to the original

Very close by design, per its README and visible structure:
* each function keeps its original address as an `// OFFSET:` tag;
* original module and file names are kept;
* Win32 builds are compared for struct layout and x87 floating point
  ("keeps Win32 code comparable with the binary", `CMakeLists.txt`);
* the history includes a "verification harness" and "VERIFY stamps"
  (commit subjects of 2026-09-23).

### 2.4 Dependence on Blizzard binaries and data [verified]

* **Game data**: it requires "a World of Warcraft 3.3.5a (12340) client. No
  game data is included" (README). Everything it renders or plays comes from
  MPQ archives: M2, WMO, ADT, BLP, DBC, FrameXML and GlueXML.
* **Win32 builds** need `DivxDecoder.dll` copied from the retail client
  (README); x64 uses a replacement decoder (`Tools/XvidDecoder`).
* **Integrity checks**: the client checks MPQ and `.toc.sig` signatures unless
  built with `WOWDECOMP_DISABLE_SIGNATURE_CHECKS` (`CMakeLists.txt`).
* **Anti-cheat**: it contains the Warden anti-cheat client
  (`WoW/Source/Game/GameClient/Win64/WardenClient.cpp`, 114 lines).
* **Interface code**: there is no Lua, XML or TOC interface code in the
  repository (0 files [verified]). The whole stock UI (unit frames, cast bars,
  raid frames, aura buttons) is FrameXML Lua/XML inside the retail data. The
  repository has only the C++ widget engine and script bindings that run it.

### 2.5 Self-contained parts

Only the third-party libraries in `ThirdParty/`, which are upstream sources
with their own licenses:
* zlib, bzip2 and expat;
* FreeType (FTL or GPLv2);
* PCRE (BSD), LZMA, Logitech `lgLcd`;
* NVIDIA nvapi (MIT per `ThirdParty/nvapi/License.txt`);
* **xvidcore under GPLv2** (`ThirdParty/xvidcore/licenses/xvid.txt`), used by
  `Tools/XvidDecoder`.

Everything under `Engine/` and `WoW/` is reconstructed. That includes several
libraries Blizzard linked statically, which the README partly describes
differently:

| Path | OFFSET tags [verified] | Notes |
|---|---|---|
| `Engine/Source/lua-5.1` | 506 | |
| `Engine/Source/jpeg` | 148 | |
| `Engine/Source/SoundEngine/fmodex/src` | 1,249 | ships with Firelight's *non-commercial* FMOD Ex license text (`fmodex/LICENSE.TXT`) |

The README says parts of FMOD Ex are "linked from their upstream sources
instead of being decompiled". The tags show at least much of
`fmodex/src` is reconstructed.

### 2.6 License and authorization [verified]

* There is no `LICENSE` or `COPYING` at the root, and no license statement in
  the README.
* `gh api repos/SatyPardus/wowdecomp` returns `"license": null`.
* No file claims ownership or grants redistribution or reuse rights for the
  reconstructed code.
* The credited projects carry the following licenses:

  | Project | License |
  |---|---|
  | `whoahq/whoa` | Unlicense |
  | `thunderbrewhq/thunderbrew` | Unlicense |
  | `thunderbrewhq/binana` (binary analysis) | none |

  Thanks to them in the README does not license wowdecomp's code.

Without a license, default copyright applies: nobody may reuse it. Beyond that,
the code reconstructs a proprietary program, so its author very likely could
not license it even if they wanted to.

### 2.7 Technical constraints for a Rust/Bevy application

Even ignoring the legal side, wowdecomp doesn't fit our application:
* **Platform and graphics**: Windows-only C++17 with MSVC specifics, and a
  Direct3D 9/OpenGL fixed-function-era renderer (`Engine/Source/Gx`). Bevy
  renders with wgpu (Vulkan, DX12, Metal) and has its own ECS, asset and
  render architecture.
* **Data formats**: everything is shaped around Blizzard formats (MPQ, M2,
  WMO, ADT, BLP, DBC) and a networked object mirror (`ObjectMgrClient`,
  `Net`).
* **Deep coupling**: the Gx device, Storm memory and containers, and global
  state make any piece hard to lift out [inference from the module graph in
  `CMakeLists.txt`].
* **FFI** from Rust would drag in the whole engine and the proprietary-code
  problem.

---

## 3. Architecture overview of the 3.3.5a client (as the tree shows it)

Function names below come from `// OFFSET:`-tagged definitions [verified]. They
describe structure only; no implementation was studied for reuse.

| Layer | Modules | Role |
|---|---|---|
| Platform | Storm, Os, Event, BlizzardCore, Tempest | memory, containers, files, threads, input, math |
| Graphics device | Gx (`CGxDevice`, D3D9 and OpenGL back ends), Gxu, ShaderEffect, FFX | state, buffers, shader permutations, glow and fog post-passes |
| Models | Model2 (`CM2Model`, `M2Cache`, `CM2SceneRender`, `M2Light`) | animated creature, character and doodad models: bones, sequences, attachments, particles, ribbons |
| World | WorldClient (`CMapObj`, `CMapObjGroup`, `CPortalView`, `CWorldOcclusion`, `MapChunk`, `DayNight`, `MapWeather`, `LiquidSystem`) | terrain tiles, buildings and dungeons as WMO groups with portals and BSP, doodads, lights, fog, weather, liquids |
| Objects | Object (`CGUnit_C`, `CGPlayer_C`, `CMovement_C`, `CMoveSpline`, game object types) | a client mirror of server objects, local player movement, remote-unit interpolation |
| Spells | Magic (`Spell_C`, `SpellVisuals`, missiles, chain lightning) | client cast prediction, cast bars, visual kits, missiles, camera shakes |
| UI | Frame, FrameScript, FrameXML, Ui, UIUtil | a retained widget engine with Lua, plus bindings for the stock interface (in game data) |
| Network | Net, Battlenet, WowServices | the realm and world protocol |

---

## 4. Subsystems relevant to Dungeon Sandbox

For each: what the repository shows, what that suggests, and what it means for
us. Function names are cited only as evidence that a subsystem exists.

### A. World and dungeon rendering

**What the repository shows** [verified names]:
* **Buildings and instances** are WMO "map objects" split into **groups**.
  `CMapObj::InteriorRender`, `ExteriorRender`, `RenderThruPortals`,
  `AddInteriorPortalView`, `AddExteriorPortalView`, `TransformPortal`,
  `ClassifyPortalPlane` and `CalcPortalFarthestDistance` show a
  cells-and-portals visibility pass: from the camera's group, draw what is
  visible through doorway polygons.
* **Occluders**: `WorldClient/CWorldOcclusion.cpp` adds occluder volumes.
* **Collision is separate from the render batches**: each group has a BSP for
  collision and segment queries (`BspQuery_Segment`, `CMapObjGroup::GetTris`).
* **Lighting**: indoor groups have their own lighting
  (`CMapObjGroup::SetLighting`, `CMapObj::SetEmissiveAmbient`), and fog comes
  from a day/night and zone light system (`SetShaderFogFromDayNight`,
  `DayNight`, `MapLight`).
* **Liquids** have their own material system (magma, procedural water:
  `LiquidSystem/MaterialMagma.cpp`, `MaterialProcWater.cpp`).
* **Detail doodads** (ground clutter) are batched instances
  (`CDetailDoodadInstBatch`).

**What that suggests** [inference]:
* Hand-built dungeons are composed of reusable groups, each with three things:
  render geometry, collision geometry and portal polygons to neighbours.
* The renderer floods visibility through portals each frame.
* Lighting and fog change between interior and exterior groups.

**Independent sources**: cells-and-portals visibility is standard computer
graphics:
* Teller and Séquin, "Visibility preprocessing for interactive
  walkthroughs", SIGGRAPH 1991;
* Luebke and Georges, "Portals and mirrors", I3D 1995;
* the visibility chapters of *Real-Time Rendering*.

(Cited from general knowledge; not fetched in this session.)

**For us**:
* Our dungeons already *are* cells and portals, implicitly:
  * Dungeon Forge knows every room, every corridor and every joint;
  * navigation portals are where areas overlap (RULES.md, Navigation).
* What we lack:
  * art per component: rooms are procedural boxes and polygons (`cave.rs`,
    `dungeon.rs`);
  * per-room visibility: everything is drawn;
  * per-room light and fog: one fog per dungeon (`dungeon.rs`), point lights
    per room.
* Collision stays authored. Our navigation is the authority and art may never
  change it (ASSET_PIPELINE_RESEARCH.md §1.5). That matches the client's split
  between render geometry and collision geometry.

### B. Movement and navigation

**What the repository shows** [verified names]:
* **Local player movement** (`WoW/Source/Object/Movement.cpp`, 256 functions):
  * a collision volume tested against world facets: `BuildCollisionBox`,
    `BuildCollisionVolumePlanes`, `IsFacetWall`, `IsFacetOverhead`;
  * step-up height (`GetStepUpHeight`, `GetRemainingStepUpHeight`);
  * sliding off steep slopes (`ComputeSteepSurfacePushNormalImpl`) and
    obstacle edges (`PushOffObstacleEdge`, `CalcRunWalkBlockingObstaclePush`);
  * falling, swimming and flying states;
  * a queue of movement events sent to the server (`CPlayerMoveQueue`,
    `CPlayerMoveEvent`), with time-skew samples (`AddTimeSkewSample`,
    `SendTimeSyncResp`).
* **Remote units** (NPCs, other players):
  * `CGUnit_C::HandleMonsterMovePacket` receives `SMSG_MONSTER_MOVE`
    (`WoW/Source/Object/ObjectClient/Unit_C.cpp`);
  * `CMoveSpline` holds a server-sent spline;
  * the client interpolates along it (`CMovement_C::Interpolate`,
    `SetSplineFaceTarget`, `SetSplineFacePoint`).
* **No NPC pathfinding**: there is no navmesh, A* or creature-AI code. A
  search for pathfinding names finds only taxi routes and transport ship paths
  (`CGTaxiMap::GeneratePaths`, `ShipPath`).

**What that suggests** [inference]:
* The client owns *how the local player moves* (collision response, step-up,
  slope sliding).
* The server decides *where NPCs go* and sends splines; the client only plays
  them back smoothly.

**For us**:
* Our simulation owns all movement and moves actors on validated navigation
  surfaces. There is no physics collision, and none is needed for
  correctness.
* The useful lessons are about *feel*:
  * a maximum step height;
  * sliding along walls instead of stopping (we clamp to the walk polygon);
  * smoothing facing changes;
  * playing back authoritative paths with interpolation. We already
    interpolate rendered positions between 20 Hz ticks (`dungeon.rs`,
    `prev_pos`); facing and foot height are not interpolated.

### C. Character models and animation

**What the repository shows** [verified names, `Engine/Source/Model2/M2Model.cpp`,
163 functions]:
* skeletal animation with sequences and variations (`FindSequenceIndex`,
  `FindSequenceVariationCount`, deferred sequences);
* sequence and event callbacks (`SetSequenceCallback`, `SetEventCallback`);
* **attachment points** (`GetAttachmentPosition`,
  `GetAttachmentWorldTransform`, `AttachToParent`);
* billboarded bones, particles and ribbons per model;
* a shared model cache (`M2Cache`);
* animation split into single-threaded and multi-threaded steps
  (`AnimateST`, `AnimateMT`, `AnimateParticlesMT`).

Character composition sits in `CCharacterComponent`: helm and shoulder model
paths, which points to equipment as attached models.

**What that suggests** [inference]: characters need four things — a skeleton,
named animation sequences, named attachment bones and animation events (for
footsteps, weapon swings and spell release). Everything visual (spells,
weapons) hangs off attachments.

**For us**:
* Characters are capsules (`dungeon.rs`: `Capsule3d`).
* Bevy 0.20 already provides what a simplified version needs [verified:
  examples at v0.20.0]:
  * glTF skinned meshes (`examples/animation/animated_mesh.rs`);
  * blending through `AnimationGraph` (`animation_graph.rs`);
  * per-bone masks for upper-body casting while running
    (`animation_masks.rs`);
  * animation events (`animation_events.rs`, `animated_mesh_events.rs`);
  * morph targets.
* No WoW technique is needed. What's needed is original rigged art and a small
  state machine driven by simulation state.

### D. Combat presentation and UI

**What the repository shows** [verified]:
* **Data is mirrored from the server**:
  * casts (`SMSG_SPELL_START` and `SMSG_SPELL_GO` in
    `Magic/MagicClient/Spell_C.cpp`);
  * auras (`SMSG_AURA_UPDATE`, `SMSG_AURA_UPDATE_ALL`);
  * threat (`SMSG_HIGHEST_THREAT_UPDATE`, `SMSG_THREAT_CLEAR`,
    `SMSG_THREAT_REMOVE` in `Object/ObjectClient/Unit_C.cpp`).
* **Threat display**: `CGUnit_C::CalculateThreat` and `GetThreatSituation`
  keep a client copy of threat lists for display and warnings
  (`CGGameUI::ThreatMapAdd`, `UpdateThreatWarningEnabled`).
* **Spell visuals** (`Magic/MagicClient/SpellVisuals.cpp`): data-driven visual
  kits with model-attach lookups, missiles (`Missile_C`,
  `UnitMissileTrajectory_C`), chain lightning objects and camera shakes per
  spell effect.
* **UI layout and behaviour are absent**: the stock frames (party and raid
  frames, cast bars, aura buttons, nameplate layout) are FrameXML Lua/XML in
  the game data. Only the widget engine and bindings are in the repository.
* **Floating combat text**: `CGUnit_C::AddWorldThreatUpdateText` and
  `WorldText` exist.

**For us**:
* We already have, in `hud.rs`, `overlay.rs` and `tooltip.rs`:
  * clickable party and raid frames;
  * threat-coloured nameplates with an enemy cast line that marks
    interruptible casts "(kick!)" (`overlay.rs`);
  * floating text, the player's cast bar, an action bar and generated
    tooltips.
* The gaps worth closing:
  * **graphical cast progress on enemy nameplates**: today it's a text line,
    which is hard to time an interrupt against;
  * **aura icons with durations**: auras are text today (already in TASKS);
  * **ability visuals tied to attachment points**: cast glow at the hands,
    missile, impact.
* Nothing in wowdecomp is needed for these.

### E. Asset management

**What the repository shows** [verified names]:
* models through a shared cache (`M2Cache`);
* textures through BLP loaders and `CGxTex`;
* client tables through generated DB records (`WoW/Source/DB`, 496 files);
* asynchronous loading (`CMapObjGroup::AsyncFailedCallback`,
  `CM2Model::WaitForLoad`);
* a texture budget (`CWorld::UpdateTextureCacheBudget`);
* everything rooted in the MPQ archive chain (Storm).

**For us**:
* Bevy's `AssetServer` already provides handles, caching, async loading and
  hot reload. glTF is our format.
* At our scale (dungeons under 300 m, under 100 areas, a 30 MB package):
  * streaming and budgets are unnecessary;
  * organisation is what matters: per-component prefabs, theme palettes,
    decoration anchors and a manifest with a size budget
    (ASSET_PIPELINE_RESEARCH.md §4 already plans these).

---

## 5. Client/server responsibility analysis

| Subsystem | Where it lives in WoW | What wowdecomp establishes | What remains unknown from this source | Relevance to our standalone sim |
|---|---|---|---|---|
| Combat resolution (hit tables, damage) | Server | Client receives results (`SMSG_SPELL_GO`, combat log `UnitCombatLog_C`) | Server formulas | None; RULES.md is our authority |
| Threat | Server | Client mirrors threat lists for UI (`CGUnit_C::UpdateThreat*`, `SMSG_*THREAT*`) | How threat is computed | Only UI ideas (threat warnings we already show) |
| Enemy AI, boss scripts, encounters | Server | Nothing: only `DungeonEncounterRec` table records and `Ui/InstanceEncounter_C` UI | Everything | None |
| NPC pathfinding | Server | Client plays server splines (`CMoveSpline`, `HandleMonsterMovePacket`) | Pathfinding entirely | None; our Nav and AI are our own |
| Player movement and collision | Client (server validates) | Collision volume vs facets, step-up, slope push, falling and swimming states (`Movement.cpp`) | Server-side validation rules | Feel lessons only (§4B) |
| Remote-unit interpolation | Client | Spline playback, facing toward target or point | — | Small: smoother facing, foot height |
| Third-person camera | Client | Smoothing of yaw, pitch, height and FOV, collision query flags, terrain tilt, shakes (`UIUtil/Camera.cpp`, 130 functions) | — | Moderate: camera feel (§9, item 7) |
| Indoor rendering and visibility | Client | Portal views, interior/exterior passes, occluders | — | Concept relevant (§9, item 2) |
| Lighting and fog | Client | Day/night and zone lights, interior group lighting, fog passes | — | Concept relevant (§9, item 3) |
| Models and animation | Client | Bones, sequences, events, attachments, particles, ribbons | — | Concept relevant (§9, item 4) |
| Spell visuals | Client | Visual kits, missiles, chain effects, camera shakes | — | Concept relevant (§9, item 5) |
| UI frames | Client (FrameXML in game data) | Widget engine only; layouts absent | Stock frame behaviour | Little; ours is Bevy UI |
| Assets | Client | MPQ, BLP, M2, WMO, DBC loaders and caches | — | None (formats we must not use) |

---

## 6. Comparison with Benilla

| | wowdecomp | Benilla |
|---|---|---|
| What it is | Reconstruction of Blizzard's 3.3.5a executable, function by function | A from-scratch Rust/Bevy client for 1.12.1 that talks to servers and reads the user's retail data |
| Provenance | Derived from `Wow.exe` (addresses on every function) | Own code. Its METHOD says "MIT OR Apache-2.0 … a claim about provenance: no original client code". Behaviour is specified from benilla-re, a published RE of the 1.12.1 client (Apache-2.0 per the GitHub API), cited by address (about 24,000 address-citing comment lines [verified count]) |
| License | None | MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`); vendored kira and a patched Lua keep their own |
| Language and engine | C++17, D3D9/OpenGL, MSVC, Windows only | Rust, Bevy **0.18.1** (`Cargo.toml`), avian3d 0.6 for collision, kira audio; macOS, Linux, Windows |
| Rendering | Original fixed-function-era pipeline | Bevy renderer with custom shaders: portal-culled WMOs (`crates/benilla-world/src/wmo_portal/`), interior lighting, GPU-skinned M2, particles and ribbons |
| Asset loaders | Original MPQ, BLP, M2, WMO, ADT, DBC code | Own Rust readers for the same formats, Bevy-free crates plus a Bevy adapter (`benilla-assets`) |
| Camera and collision | Original | `benilla-app/src/player/camera.rs` (collision-swept boom, fade to first person); `benilla-world/src/collision.rs` (separate colliders for body and camera, because the client filters faces per query) |
| Game data needed | Retail 3.3.5a `Data` | Retail 1.12.1 `Data` |
| Bevy 0.20 compatibility | n/a | Not directly: 0.18 → 0.20 crosses two releases of breaking API changes |
| Applicability to us | Not usable (§8) | Architecture reference; code legally reusable with attribution but tied to WoW formats and Bevy 0.18 |

Precision on the difference:
* **Benilla** re-implements observed and reverse-engineered behaviour in new
  code, under a permissive license. Its module docs describe the 1.12 client's
  algorithms with addresses (for example, the portal flood with screen-rect
  narrowing in `wmo_portal/mod.rs`, and the per-audience collision filtering
  in `collision.rs`).
* **wowdecomp** *is* the original program's logic, re-expressed in C++.

**Is each technique available from a better source?** Yes, for everything we
want:

| Technique | Better source |
|---|---|
| Cells and portals | textbooks and papers (§4A) |
| Skeletal animation, blending, events | Bevy 0.20 itself, with its examples |
| Camera booms with collision sweeps | general third-person camera practice; our own `camera.rs` already does ray probes |
| Data-driven visual effects | general practice, and Bevy's built-in features |

We don't need either WoW project as a source of these.

---

## 7. Compatibility with our Rust/Bevy architecture

Our boundaries, unchanged:
* `dungeon_sim` is deterministic and Bevy-free, and owns all rules and
  movement.
* `dungeon_forge` is Bevy-free and produces validated TOML.
* `dungeon_game` renders, handles input and runs the UI.

Every recommendation below lives in `dungeon_game`, or as data in `dungeon_forge`
kits. None touches simulation determinism, navigation authority or the combat
model.

| Concern | Assessment |
|---|---|
| Linking wowdecomp C++ | Not possible legally (§8). Technically: Windows-only, D3D9, global-state engine |
| Using Benilla crates | License OK; technically they read WoW formats only, and they target Bevy 0.18 with a curated feature set; mixing in avian3d adds physics we don't need |
| Bevy 0.20 features we'd use | glTF scenes; skinned meshes; `AnimationGraph`, masks and events; `Visibility` and `VisibilityRange`; `DistanceFog`, `FogVolume`, light probes; clustered decals; bloom; occlusion culling (examples verified to exist in v0.20.0) |
| Determinism | Unaffected: visibility, animation and effects read sim state and never write it |
| Navigation authority | Kept: prefabs are checked against the walkable footprint, as props are today |

---

## 8. Licensing and intellectual-property considerations

**This is not legal advice.** Where the boundary is unclear, the
recommendation is independent development or legal review.

| Category | Material | Reasoning |
|---|---|---|
| **A — Independently usable** (subject to normal compliance) | Benilla's own code (MIT OR Apache-2.0) | Clear permissive license. Not needed: tied to WoW formats and Bevy 0.18. Any adaptation would need attribution, a dependency review and a RESEARCH.md entry |
| **A** | Upstream libraries in `wowdecomp/ThirdParty` (zlib, bzip2, expat, FreeType, PCRE, LZMA, nvapi under MIT) | Their own licenses apply. We have Rust equivalents and need none |
| **A — not compatible with our licensing** | `ThirdParty/xvidcore` | GPLv2: incompatible with distributing our MIT/Apache build. Irrelevant to us anyway |
| **B — Technical and behavioural reference** | General concepts visible in module and function *names* and in public literature, as listed below | Ideas and standard techniques, not expression. Build them from independent sources (papers, Bevy docs) and our own design |
| **C — Needs legal clarification** | Benilla's *behavioural specifications*: the address-cited client facts and benilla-re's tables (Apache-2.0, but derived from the 1.12.1 executable) | The license on their text is clear; how far facts derived from a proprietary binary are free to reuse is a legal question. We have no need for them |
| **C** | FMOD Ex material in `wowdecomp/Engine/Source/SoundEngine/fmodex` | Firelight's non-commercial license text with source reconstructed from Blizzard's linked copy. Unclear and irrelevant |
| **D — Not appropriate** | All of `wowdecomp/Engine/` and `wowdecomp/WoW/` | Reconstructed Blizzard code, no license, a derivative of a proprietary executable |
| **D** | Anything expressing its algorithms: translations to Rust, shader logic, constants or tables lifted from it | Translating decompiled functions is still copying expression and must not be done |
| **D** | Blizzard formats and data (MPQ, M2, WMO, BLP, DBC, FrameXML) and any loader for them in our game | Our rules forbid game-extracted assets and any client dependency (CLAUDE.md) |
| **D** | The Warden client and signature-check bypass options | Not for us |

The Category B concepts are:
* cells and portals for indoor visibility;
* separate render and collision geometry;
* per-group interior lighting;
* skeletal sequences with attachment points and events;
* client-side spline playback of authoritative paths;
* camera smoothing and collision with "camera ignores this" geometry flags;
* data-driven spell visual kits;
* step height and slope sliding.

Notes:
* Lack of a license is not permission. wowdecomp's 83 stars and 41 forks
  [verified] say nothing about rights.
* Reading wowdecomp for this assessment was limited to repository metadata,
  READMEs, build files, the file and module layout, function *names* and
  license texts. No function bodies were studied as a design source.
* Future implementation work should continue to avoid the repository, so
  that our code stays clearly independent. **[Recommendation]** record a
  clean-room note in RESEARCH.md: future contributors working on rendering
  should not consult wowdecomp source bodies.

---

## 9. Potential improvements, ranked by value for making and playing dungeons

The rank reflects how much each helps create interesting dungeon content and
play it, not whether WoW has it. Complexity uses this scale:
* **S**: up to about a week;
* **M**: one to three weeks;
* **L**: longer, or blocked on art.

### 1. Modular room prefabs with separate visual and walkable geometry
| | |
|---|---|
| Description | Kits name a GLB prefab per component (hall with pillars, arena, corridor straight, stair, bridge deck, doorway frame) with decoration anchors; the composer places it at the component's transform; walkable geometry stays the areas Forge generates; procedural geometry is the fallback |
| Current limitation | Every room is procedural boxes and polygons; decoration is hard-coded in `compose.rs`/`cave.rs`; dungeons look like graybox |
| Research | WMO groups combine render batches, separate collision BSP and doodad sets placed by definition (§4A). Benilla bakes collision per audience. Our 0.6 plan (ASSET_PIPELINE_RESEARCH.md §4.1–4.3) |
| Benefit | The biggest visual step for every generated layout at once; designers think in rooms; Forge stays the authority |
| Complexity | M for the system; L overall because of original art |
| Bevy 0.20 | glTF scenes, `SceneRoot`; no new dependencies |
| Legal | Independent; original art with provenance rows |
| Original assets | Yes (Blender, or scripted like `make_prop_models.py` for simple pieces) |
| Performance | More meshes; merge static meshes per room and pair with item 2 |
| When | 0.8 (presentation milestone) |

### 2. Room-and-doorway visibility from the Forge plan
| | |
|---|---|
| Description | Build a room graph from the plan (rooms, corridor cells, doorway rectangles) or, for handwritten dungeons, from navigation portals grouped by area name. Each frame, flood from the camera's room through doorways that intersect the narrowed view, and hide room entities that aren't reached |
| Current limitation | Everything is drawn. Fine today (dungeons are small), but it blocks denser art and per-room effects |
| Research | Interior and exterior portal views and occluders (§4A); Benilla's `wmo_portal` flood; the literature |
| Benefit | Headroom for prefab art, lights and particles; also gives a "current room" signal for lighting, fog, ambience and music |
| Complexity | M |
| Bevy 0.20 | `Visibility` on per-room parent entities; camera frustum math in Bevy; optionally `VisibilityRange` |
| Legal | Independent (textbook algorithm) |
| Original assets | No |
| Performance | Positive when art is heavy; the flood is cheap (rooms < 100) |
| When | Prototype in 0.8 (PoC 2); adopt only if measured gains justify it |

### 3. Per-room lighting and fog
| | |
|---|---|
| Description | Kit palettes define light rigs and fog per room role (entrance, hall, arena, pit), blended as the camera moves between rooms, plus local fog volumes (smoke over lava, crypt mist) and light probes |
| Current limitation | One `DistanceFog` per dungeon (`dungeon.rs`); point lights per room with fixed intensity |
| Research | Interior group lighting, the day/night zone light system and fog passes (§4A) |
| Benefit | Atmosphere and readability: arenas feel like arenas, crypts feel cold, routes are lit |
| Complexity | S–M |
| Bevy 0.20 | `DistanceFog`, `FogVolume` (`fog_volumes.rs`), light probes, bloom |
| Legal | Independent |
| Original assets | No |
| Performance | Fog volumes cost GPU; budget per dungeon |
| When | 0.8 |

### 4. Animated GLB characters with attachment points
| | |
|---|---|
| Description | One original rigged humanoid (plus one or two creature rigs) with clips idle, run, cast, channel, attack, hit, stun and death; an animation state machine driven by sim state; upper-body masks so casting works while moving; named attachment bones for weapons and effect origins |
| Current limitation | Capsules; facing and actions shown only by text and effects |
| Research | M2 sequences, events and attachments (§4C); Bevy has every needed feature |
| Benefit | Readability of who is casting, attacking or stunned; feel; a prerequisite for item 5 |
| Complexity | M for code; L for art |
| Bevy 0.20 | `AnimationGraph`, `AnimationMask`, animation events (examples verified) |
| Legal | Independent code. Art must be original or properly licensed (e.g. CC0 packs with provenance) |
| Original assets | Yes |
| Performance | Skinning cost for 15–30 characters is modest; measure with 10-player raids |
| When | 0.9 |

### 5. Data-driven ability visuals
| | |
|---|---|
| Description | Abilities and enemy abilities reference visual kits (cast glow at an attachment, missile, impact, ground decal, chain, camera shake) defined in content data, rendered from sim events |
| Current limitation | `fx.rs` colours effects by school with generic shapes |
| Research | Spell visual kits, missiles, chain lightning, camera shakes (§4D) |
| Benefit | Telegraph clarity: players read mechanics such as the wail, frontal cones and ring timing; bosses feel distinct |
| Complexity | M |
| Bevy 0.20 | Meshes, materials and clustered decals built in; a particle crate (for example bevy_hanabi) would need explicit dependency approval, otherwise simple CPU particles |
| Legal | Independent |
| Original assets | Some (textures and meshes); can start procedural |
| Performance | Budget particles; cull with item 2 |
| When | 0.9 (after characters, for attachments); simple decals earlier |

### 6. Standalone dungeon viewer
| | |
|---|---|
| Description | `dungeon-sandbox --view <file.toml>`: renders a dungeon with no simulation; fly camera; toggles for nav overlay, room graph, encounter markers, patrol routes, report overlays; hot reload on file change |
| Current limitation | Inspection needs `--demo` plus `--area-tour` (a running sim), or SVG maps |
| Research | Benilla's `benilla-worldview` (engine without a game); `wowdecomp/CMakeLists.txt` names `MapViewer` and `M2Viewer` targets in a loop, but no such targets exist in the tree [verified] |
| Benefit | Fast art and Forge iteration; a foundation for a graphical editor |
| Complexity | S–M |
| Bevy 0.20 | Yes; reuses `dungeon.rs` and `cave.rs` builders |
| Legal | Independent |
| Original assets | No |
| Performance | n/a |
| When | 0.8 (enables items 1–3) |

### 7. Camera feel: smoothing, collision with authored geometry, camera-transparent decor
| | |
|---|---|
| Description | Collide the camera against the same wall data navigation uses (not ad-hoc AABBs). Mark decor and props camera-transparent or fading. Smooth pitch and height on ramps and stairs. Add short spell camera shakes |
| Current limitation | `camera.rs` probes `CameraBlockers` boxes with five rays; on polygon caves and pillared halls boxes approximate the walls |
| Research | The client camera's collision query flags, smoothing and shakes (§5); Benilla's separate camera collider |
| Benefit | Fewer clipping and occlusion moments in tight crypt halls; better feel |
| Complexity | S |
| Bevy 0.20 | Yes (math only) |
| Legal | Independent |
| Original assets | No |
| Performance | Negligible |
| When | 0.8 |

### 8. Movement presentation: facing and foot-height smoothing, wall sliding for the player
| | |
|---|---|
| Description | Interpolate facing (turn rate) and elevation in the renderer. In the sim, slide the player's step along the wall instead of clamping (sim change: needs hash checks; behaviour change only for the player) |
| Current limitation | Positions interpolate, facing snaps; clamping can feel sticky at corners |
| Research | Client wall and overhead facets, slope push, spline facing (§4B) |
| Benefit | Smoother look and control |
| Complexity | S |
| Bevy 0.20 | Yes |
| Legal | Independent |
| Original assets | No |
| Performance | Negligible |
| When | 0.8 (rendering part); sim part with measured regression |

### 9. Encounter-critical UI: graphical enemy cast bars, aura icons with timers
| | |
|---|---|
| Description | Turn the enemy nameplate cast line into a filling bar (interruptible coloured), and show aura icons with remaining time on target and party frames |
| Current limitation | Nameplates show enemy casts as text ("Name casting X (kick!)", `overlay.rs`) without progress; auras are text lines (`hud.rs`) |
| Research | Server-mirrored casts and auras drive these in the client (§4D) |
| Benefit | Interrupt gameplay (Warden's Screen, abbots) becomes readable |
| Complexity | S |
| Bevy 0.20 | Bevy UI |
| Legal | Independent |
| Original assets | Icons (simple original glyphs) |
| Performance | Negligible |
| When | 0.8 |

### 10. Asset manifest and package budget
| | |
|---|---|
| Description | Kits list their prefabs and props; the packager reports asset sizes and fails above a budget; a loading step preloads a dungeon's handles |
| Current limitation | Props loaded ad hoc; no budget |
| Research | Client caches and texture budgets (§4E) — not needed at our scale beyond a budget |
| Benefit | Keeps the ZIP small as art arrives |
| Complexity | S |
| Bevy 0.20 | Yes |
| Legal | — |
| Original assets | No |
| Performance | Avoids hitches on first sight |
| When | 0.8 |

---

## 10. Dungeon Forge applications

The workflow goal: design structure → populate encounters → validate
navigation → simulate parties → produce a playable environment. What moves it
forward, given this research:

| Idea | Fit with Forge | Notes |
|---|---|---|
| **More sophisticated indoor geometry** | Strong | Components already exist (room kinds, pillared halls, bends). Add prefab variants per component with anchors (item 1). Keep areas as the walkable truth; validate prefab walls against the area outline (like props today) |
| **Room-based visibility** | Strong | Forge's `Plan` has rooms, cells and corridors, and `Composed.owners` maps every area to its room or corridor. Exporting a `[[dungeon.rooms]]` table (room → area names, doorway rectangles) gives the renderer a portal graph without inference (item 2) |
| **Modular environment components** | Strong | The 0.7 kit `[components]` is the hook; add `prefab = "models/kit/crypt_hall.glb"` per room spec |
| **Better collision and movement** | Moderate | Collision stays authored. Movement feel improvements are renderer and player-side (items 7–8) |
| **More natural cave layouts** | Moderate | The real limit is the grid (DUNGEON_FORGE_0_7.md). Neither repository helps: WoW's caves are hand-modelled. Organic shells (prefab walls following jittered outlines) give much of the look without leaving the grid |
| **Environment prefabs** | Strong | as above |
| **Rendering performance** | Later | Measure first; prefabs plus visibility |
| **Character model placement** | Moderate | Forge already places packs with formations and facing (`placement.rs`); animated characters (item 4) make formations visible |
| **Automated dungeon visualization** | Strong | Viewer (item 6) plus the existing `--area-tour`, nav-debug and report SVG; add "screenshot every room" from the room table for review sheets |
| **Graphical dungeon editing** | Long term | Viewer → selection → overrides (the 0.7 PoC) as the edit format; no rewrite needed |

---

## 11. Recommended independent prototypes (not implemented here)

### PoC 1 — Modular room prefab with separate walkable geometry
* **Objective**: show that a GLB prefab can replace the procedural look of one
  Forge component (the crypt's pillared hall) while navigation, encounters and
  replay hashes stay unchanged.
* **Scope**:
  * one prefab with walls, pillars, floor trim, two door frames and four
    decoration anchors, generated by script or authored in Blender (original,
    with a provenance row);
  * a kit field `prefab` on `[components.rooms.hall]`;
  * the composer emits a `[[dungeon.props]]`-like room prefab;
  * the renderer skips procedural walls for that room;
  * a validator checks that prefab bounds don't intrude into walkable areas,
    reusing the prop footprint rule plus a per-prefab outline.
* **Dependencies**: none new (Bevy glTF).
* **Challenges**:
  * aligning prefab scale and pivot to component extents, which vary per
    seed (jitter, aspect): either 9-slice style pieces or per-extent variants;
  * doorway positions vary with corridor offsets.
* **Success criteria**:
  * `--area-tour` screenshots of 3 crypt layouts show the prefab with no
    gaps or overlaps at doors;
  * identical `log_hash` for 100 seeds against the pre-PoC build;
  * frame time within 10%.
* **Measuring value**: before/after screenshot sheets reviewed by a person;
  authoring time per new prefab.

### PoC 2 — Room-and-doorway visibility from the Forge plan
* **Objective**: measure whether portal-flood visibility reduces rendered
  entities and frame time on generated dungeons, with no visible popping.
* **Scope**:
  * export a room table from Forge (room → areas, doorway rectangles);
  * in the game, parent each room's meshes under a room entity;
  * a per-frame flood from the camera's room, narrowing a screen-space rect
    through doorway rectangles (textbook method);
  * toggle `Visibility`;
  * a `--vis-debug` overlay.
* **Dependencies**: none.
* **Challenges**:
  * the camera outside walls (camera collision);
  * big open rooms;
  * handwritten dungeons without a room table: fall back to "draw all".
* **Success criteria**:
  * on the crypt with stress decor, at least 40% fewer visible meshes on
    average along the demo route;
  * the flood costs under 0.1 ms;
  * zero frames where a visible room is hidden (automated check: render with
    and without culling at sampled camera poses; compare masks).
* **Measuring value**: frame-time traces of `--demo` runs at 5 and 10
  players, against a heavier art budget.

### PoC 3 — One animated original character driven by simulation state
* **Objective**: establish the character pipeline (rig → clips → Bevy
  `AnimationGraph` → sim-driven state machine → attachments) before any art
  investment.
* **Scope**:
  * one humanoid GLB with idle, run, cast-loop, attack, hit and death. It is
    original, or a CC0 asset with a provenance row (the asset choice needs
    approval);
  * used for one class in the party;
  * a weapon attached to a hand bone;
  * cast glow spawned at the hand attachment;
  * upper-body mask for casting while moving.
* **Dependencies**: none new.
* **Challenges**:
  * foot sliding: match clip speed to sim move speed;
  * blending timings at 20 Hz sim ticks with render interpolation;
  * scale against the 0.5 m actor radius.
* **Success criteria**:
  * correct state in an automated `--frame-test`-style check (sampled
    frames: casting actor plays cast);
  * foot slide under a threshold measured from root motion vs sim speed;
  * frame time for a 10-player raid within 10% of capsules.
* **Measuring value**: readability review with screenshots; the art cost per
  additional rig.

(A camera-feel experiment was considered; it is small enough to do directly
under item 7 without a PoC.)

---

## 12. Features we should explicitly avoid

* Any code, translation or close paraphrase from wowdecomp's `Engine/` or
  `WoW/` trees, including shaders, constants, tables and algorithms as
  expressed there.
* Blizzard formats and data: MPQ, BLP, M2, WMO, ADT, DBC loaders, or any
  requirement for a client install.
* WoW's network protocol, realm login, server emulation and Warden.
* FrameXML/Lua UI hosting: our UI is Bevy-native and data comes from our
  sim.
* Open-world systems: terrain streaming, outdoor day/night cycles, weather
  systems beyond per-room effects, taxis, transports, vehicles, auction house,
  battlegrounds, quests.
* Physics-engine collision for movement: our navigation is authored and
  validated; art must never change walkability.
* Exact retail parity as a goal.

## 13. Implications for 0.8 and beyond

**0.8 — presentation (already planned in ASSET_PIPELINE_RESEARCH.md §4)**,
refined by this research:
1. Standalone dungeon viewer (item 6). Do it first; it speeds everything
   else.
2. Room table export from Forge, plus PoC 2. Adopt visibility only if it
   measures well.
3. Prefabs per component with anchors (item 1, PoC 1 first), starting with
   the crypt hall and the volcanic arena.
4. Per-room light rigs and fog volumes (item 3).
5. Camera collision against nav walls; camera-transparent props; facing
   smoothing (items 7–8 rendering side).
6. Graphical enemy cast bars on nameplates; aura icons with timers (item 9).
7. Asset manifest and package budget (item 10).

**0.9 — characters and ability visuals**: PoC 3, then rigs per class and a
creature set; the ability-visual data layer on attachments (items 4–5).

**Later**: graphical editing on top of the viewer and the overrides format.

Throughout:
* the simulation, navigation and Forge validation stay unchanged;
* replay hashes guard every presentation change that touches the sim
  (they should not);
* every asset gets a provenance row.

---

## Recommendation

**Should Dungeon Sandbox use anything from wowdecomp? No: no code, no data, no
assets, no translated algorithms, and no dependency.**
* It is an unlicensed reconstruction of Blizzard's proprietary client
  (Category D).
* It needs Blizzard data to do anything.
* The systems that would matter most to our game (combat, threat, AI,
  encounters, pathfinding) are not in it, because they live on the server.

What we can legitimately take is the confirmation, from its structure and
from independent sources, that a small set of **general techniques** carried
WoW's dungeons:
* rooms with separate render and collision geometry;
* portal visibility;
* per-room lighting;
* skeletal models with attachment points and events;
* data-driven visual kits;
* a smoothed camera.

Each should be built independently, under these conditions:
* from public literature and Bevy 0.20's own features;
* with original or properly licensed art and provenance rows;
* without consulting wowdecomp's function bodies;
* recorded in RESEARCH.md;
* measured with our existing tools (replay hashes, area tours, frame time).

**Benilla** stays an architecture reference. Its code is permissively licensed
but tied to WoW formats and Bevy 0.18; any future adaptation needs an explicit
dependency and license review first. Nothing from it is needed for the plan
above.

**Next step**: approve 0.8 to start with the dungeon viewer and PoC 1 and
PoC 2, then decide on prefabs and visibility from their measurements.

---

### Appendix: how the inspection was done
* Commands:
  * `git clone --depth 1` of both repositories into a scratch folder outside
    this repository; `git fetch --deepen=300` for wowdecomp's history;
  * `gh api repos/<owner>/<repo>` for license, dates, stars and forks; also
    for benilla-re, binana, thunderbrew and whoa;
  * file and line counts per module;
  * counts of `// OFFSET:` tags per directory;
  * function-name listings from tagged definitions;
  * `git grep` for network message handlers (`SMSG_*`) to separate server
    data from client logic;
  * license files read in full.
* Not done: building or running either client; reading any retail data;
  studying function bodies.
* Benilla files read: README, AGENTS.md, `Cargo.toml`, `docs/METHOD.md`,
  `docs/MAP.md`, `reference/README.md`, and module doc comments of
  `player/camera.rs`, `wmo_portal/mod.rs` and `collision.rs`.
* Not reachable: wowdev.wiki (HTTP 403 to automated requests); it was not
  needed.
