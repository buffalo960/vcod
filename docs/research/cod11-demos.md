# Demos: `record`, `stoprecord`, `demo`

What the retail client writes into a demo and how it plays one back, read
out of `CoDMP.exe` 1.1 (image base `0x00400000`, md5 in
`cod11-events-and-fx.md`), `CoDMP15.exe` 1.5 and `CoDUOMP.exe` 1.51. The
function names are mine, after the Quake III functions whose shape they
share. vcod's implementation is `crates/common/src/net/demo.rs` (the
format), `NetClient::start_recording` / `play_demo` / `pump_demo` in
`crates/common/src/net/mod.rs` (the record and the read), and
`crates/client/src/play/demos.rs` (file names, the automatic demo).

## 1. Commands and cvars

`CL_Init` (`0x411e60`) registers `record` with `0x40e540`, `stoprecord`
with `0x40e430` and `demo` (the string at `0x56754c`) with `0x40eb40`
(VERIFIED, the registrar `0x428840` calls). It registers the cvars
`timedemo`, `cl_avidemo`, `cl_forceavidemo` and `cl_freezeDemo` beside
them (VERIFIED, the strings). vcod has the three commands and none of the
four cvars.

- `record [name]` prints `record <demoname>` with more than one argument,
  `Already recording.` while recording, and `You must be in a level to
  record.` unless `cls.state` (`0x155f2c0`) is 6, `CA_ACTIVE` (VERIFIED,
  the three compares and strings in `0x40e540`). A name writes
  `demos/<name>.dm_1`; none tries `demos/demo0000.dm_1` up to
  `demo9999` (`0x40e4b0` formats `demo%i%i%i%i`) and takes the first that
  does not open (VERIFIED). It prints `recording to %s.`.
- `stoprecord` prints `Not recording a demo.` when not recording, else
  writes the trailer, closes the file and prints `Stopped demo.`
  (VERIFIED, `0x40e430`). `CL_Disconnect` (`0x40ef90`) calls it while
  recording (VERIFIED).
- `demo <name>` takes exactly one argument (`playdemo <demoname>`
  otherwise) and refuses on a listen server (VERIFIED, `0x40eb40`). A name
  that already ends in `.dm_1` opens `demos/<name>`, any other
  `demos/<name>.dm_1` (VERIFIED, the `.dm_%d` compare). It disconnects,
  sets `clc.demoplaying` (`0x15ef004`), sets `cls.state` 3 and reads
  messages until the state reaches 5, the gamestate parsed (VERIFIED, the
  loop on `0x155f2c0`).

The number after `.dm_` is a literal per binary: 1 in 1.1 (`0x40e540`), 2
in 1.5 (`CoDMP15.exe` `0x40f210`), 3 in UO (`CoDUOMP.exe` `0x40fb80`)
(VERIFIED, the `push` before each `demos/%s.dm_%d` format). Each retail
client reads only its own.

vcod writes `demos/` under the server's `fs_game` directory, else under
the player's mod directory, else under the base directory its config sits
in, and reads from the same list. Its `demo` plays all three: a bare name
takes the first of `.dm_1`, `.dm_2`, `.dm_3` that exists, and the number
picks the protocol the file is parsed under.

## 2. The file

A demo is a run of records closed by a trailer (VERIFIED, `0x40e3d0`
`CL_WriteDemoMessage`, `0x40e430`, `0x40e9c0` `CL_ReadDemoMessage`):

```
long  serverMessageSequence   (clc.serverMessageSequence, 0x15de99c)
long  length
byte  message[length]
...
long  -1
long  -1
```

The message is what `CL_PacketEvent` (`0x410ff0`) holds after the netchan:
the bytes from `msg.readcount` on, where `readcount` sits past the
sequence the netchan read (VERIFIED, `edi` loaded from `msg+0x10` at
`0x4110c8` and passed to `0x40e3d0`). So a record is the plain 4-byte
`reliableAcknowledge` and the op block, Huffman-compressed as it came,
the XOR scramble already undone by `0x414780`, which runs on the bytes
after the acknowledge before the parse (VERIFIED, the call at
`0x411103`). A fragmented message is written once, whole. A record
carries no time; playback paces off the snapshots inside.

`CL_ReadDemoMessage` ends the demo (`CL_DemoCompleted`, `0x40e920`) on a
short sequence read, a length of -1, or a short message read, the last
printing `Demo file was truncated.` (VERIFIED). A length past its buffer
is a fatal error (`CL_ReadDemoMessage: demoMsglen > MAX_MSGLEN`): 16384
bytes in 1.1 and 1.5, 32768 in UO (VERIFIED, the buffer sizes in
`0x40e9c0` and `CoDUOMP.exe` `0x410060`). The read stamps
`clc.lastPacketTime`, reads the acknowledge as `MSG_ReadLong` and hands
the message to `CL_ParseServerMessage` (`0x4165f0`) (VERIFIED).

## 3. The opening gamestate

`record` writes a gamestate of its own before any server message
(VERIFIED, `0x40e540`): into a 16384-byte buffer, plain,

```
long  clc.reliableSequence (0x15ce994)     where a server message has its ack
byte  svc_gamestate (2)
long  clc.serverCommandSequence (0x15de9a0)
per configstring i < 2048 with a non-zero string offset:
  byte svc_configstring (3), short i, bigstring
per entity baseline whose number is non-zero:
  byte svc_baseline (4), the delta from a null state, forced (0x445e40)
byte  svc_EOF (8)
long  clc.clientNum (0x15ce860)
long  clc.checksumFeed (0x15ce98c)
byte  svc_EOF (8)
```

then copies the first long and Huffman-compresses the rest behind it
(`0x4448b0`) and writes it as one record under the current
`serverMessageSequence` (VERIFIED). The configstrings are the client's
table as it stands, every `d` already applied. UO sizes the buffer 32768
and writes a vehicle (`eType` 12) baseline through its vehicle field
table (VERIFIED, `CoDUOMP.exe` `0x40fb80`); 1.5's is 1.1's (VERIFIED,
`CoDMP15.exe` `0x40f210`).

vcod builds it with `gamestate::write` and the trailing `svc_EOF`. Its ops
are byte for byte the retail server's gamestate past that message's
leading empty `svc_serverCommand` (VERIFIED, the committed
`gamestate.bin` in `demo.rs`'s test).

## 4. Waiting for a full frame

`record` sets `clc.demowaiting` (`0x15ef008`) (VERIFIED, `0x40e6ea`).
While it is set, `CL_WritePacket` (`0x40ba50`) sends `clc_moveNoDelta`,
the same arm as `cl_nodelta`, no valid snapshot, or a last message that
was not the snapshot's (VERIFIED, the compares at `0x40bc44`..`0x40bc6f`),
and `CL_PacketEvent` writes nothing (VERIFIED, `0x41111a`).
`CL_ParseSnapshot` (`0x415bb0`) clears it on a frame whose delta byte is
0, before it checks the frame (VERIFIED, `0x415c5f`). The write follows
the parse, so the first message written is that full frame's.

## 5. Playback

`CL_SetCGameTime` (`0x404d60`) reads the demo (VERIFIED):

- in `CA_PRIMED`, the map loaded and no snapshot yet: skip the first
  frame, then one message a frame until a snapshot makes the state 6;
- in `CA_ACTIVE`, after the clock (`realtime + serverTimeDelta -
  cl_timeNudge`, held while `cl_freezeDemo` is set): messages while
  `cl.snap.serverTime <= cl.serverTime`, stopping if the state leaves 6,
  which a gamestate in the demo does;
- `timedemo` instead steps the clock 50 ms a frame and prints the frame
  rate at the end (`0x40e920`).

`CL_AdjustTimeDelta` (`0x404bf0`) returns at once while a demo plays
(VERIFIED, its first compare on `0x15ef004`), so the delta set by the
first snapshot never slews. `CL_WritePacket` sends nothing while one plays
(INFERRED: `0x40b940` and `0x40ba6d` read `0x15ef004`).

vcod reads through `NetClient::pump_demo`: up to the first gamestate, then
nothing while the map loads, then while the newest snapshot is no later
than `ServerClock::time_at` (one snapshot past the drawn time), breaking at
a gamestate. The clock runs `ServerClock::update_demo`, which never
adjusts. It draws the recorder as a followed player off the snapshots,
as the cgame does under `cg.demoPlayback` (INFERRED from Q3's
`CG_PredictPlayerState`, not checked in `cgame_mp_x86.dll`), answers no
script menu, sends nothing, and returns to the main menu when the demo
ends, as `CL_DemoCompleted`'s `CL_Disconnect` does. It skips the
`PRIMED` one-message-a-frame stage and reads to the first snapshot at
once.

## 6. Automatic demos

1.1's cgame has no automatic demo. 1.5's `CG_ServerCommand`
(`cgame_mp15.dll` `0x3002f660`) handles `C` and `D` (VERIFIED, cases 0x43
and 0x44): with `cg_autodemo` 1 (`atol`), `C` runs `record %s-%s` through
trap 0xf5 and `D` runs `stoprecord`. The first `%s` is the `name` cvar
kept to `a-z`, `A-Z` and `0-9` (`0x3002f470`), the second trap 0xf4,
which `CoDMP15.exe` answers at `0x46af40` with the local date and time
formatted `yyyyMMdd-` and `HHmmss` (VERIFIED, the `GetDateFormatA` and
`GetTimeFormatA` pictures). UO's cgame keeps both, and its killcam's
`C <ms>` reads as a start (`coduo-diff.md`, "Server and client
commands"). The game module sends them from `autoDemoStart` and
`autoDemoStop` under `g_autodemo` (`cod15-protocol-diff.md` 8).

vcod registers `cg_autodemo` "0", archived (INFERRED: the cgame's cvar
table row was not read), and runs the same two commands on a 1.5 or UO
server.

## 7. Not done

- The `RECORDING <name>: <n>k` line `0x416b10` draws while recording.
- `timedemo`, `cl_avidemo`, `cl_forceavidemo`, `cl_freezeDemo`,
  `nextdemo`.
- A demo written by a retail client has not been played in vcod, nor a
  vcod one in a retail client: this machine runs no retail client. The
  format checks are vcod against itself (`crates/server/tests/demos.rs`)
  and the retail server's own messages played as a demo
  (`a_demo_of_retail_messages_plays_every_frame`).
