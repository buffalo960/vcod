# Bots

vcod-server can fill a server with bots. Retail Call of Duty has none, so
everything here is vcod's own. A bot is a real client on the server: it takes
a slot, joins through the stock team and weapon menus, and shows up in the
scoreboard and in `status` like anyone else. Retail clients see bots as
ordinary players.

This page covers running and managing bots. How they find their way around a
map is in [research/bot-navigation.md](research/bot-navigation.md), and how
they play each gametype's objectives is in
[research/bot-objectives.md](research/bot-objectives.md).

## Quick start

Start a server with six fighting bots:

```
vcod-server mp_carentan --gametype tdm --bots 6 --bots-shoot --set rconPassword=secret
```

Or keep a server busy on its own: with a floor of 10, an empty server runs a
5v5 of bots, and each human who joins takes a bot's place. `--max-clients`
defaults to 8, so raise it above the floor.

```
vcod-server mp_carentan --gametype tdm --max-clients 16 --set vc_bot_minplayers=10 --set rconPassword=secret
```

Everything below can also be changed on a running server over rcon, from a
retail client's console (`rcon <command>` after `rconPassword secret`) or from
vcod's.

## Command line

| Flag | Does |
|---|---|
| `--bots N` | Starts the server with N bots. Each takes a real slot, so N can't exceed `--max-clients`. |
| `--bots-shoot` | Lets those bots fire. Without it they walk the map and play objectives but never shoot (`vc_bot_shoot 0`). |
| `--set NAME=VALUE` | Sets any cvar below before the first map loads, as retail's `+set` does. |

## rcon commands

rcon is refused until the server has `rconPassword` set.

| Command | Does |
|---|---|
| `addbot [allies\|axis\|auto] [count]` | Adds `count` bots (default 1). The words can come in either order. `auto` (the default) puts each bot on whichever team balance asks for. Naming a team puts the bots there and pins them, so balance never moves them. |
| `removebot [name\|all\|count]` | With no argument, removes one bot. A name removes that bot; the `[BOT] ` prefix is optional and case doesn't matter. `all` removes every bot. A number removes that many, lowest score first. |
| `listbots` | Prints every bot's slot, team, score, whether it is pinned, and its name, then the human and bot counts and the current `vc_bot_*` values. |

Examples:

```
rcon addbot 4
rcon addbot axis 2
rcon removebot NadeSpammer
rcon removebot all
rcon listbots
```

## Cvars

Set them with `rcon set <cvar> <value>` (or `seta`), or with `--set` at
startup. A change takes effect within half a second, with no restart.

| Cvar | Default | Does |
|---|---|---|
| `vc_bot_minplayers` | 0 | The minimum player count. 0 turns it off. |
| `vc_bot_autobalance` | 1 | Keeps the teams even in team gametypes. |
| `vc_bot_shoot` | 1 | 0 stops bots from firing. `--bots` without `--bots-shoot` sets it to 0. |
| `vc_bot_names` | 1 | 0 names new bots `bot<slot>` instead of drawing a funny name. |
| `vc_bot_vehicles` | 1 | United Offensive only. 0 stops bots getting into vehicles; a bot already in one rides on until it gets out. |

## How the minimum player count works

With `vc_bot_minplayers` set to N, the server keeps N players in the game,
topping up with bots:

- With no humans, it runs N bots, split across the teams.
- When a human picks a team, one bot leaves, taken off the larger team.
- When a human leaves, a bot comes back.
- When N or more humans are playing, no floor bots remain.

A human counts toward the floor only once they have picked a team. A
spectator, or someone still in the team menu, does not push a bot out.

Bots added with `addbot` or `--bots` count separately: `removebot` lowers
that count, but the floor still tops the server back up to N.

## Team balance

With `vc_bot_autobalance 1`, in every team gametype (everything except `dm`
and 1.1's `bel`), new bots join the smaller team, and bots move so the two
teams differ by at most one player. The rules:

- Humans are never moved.
- Bots pinned with `addbot allies` or `addbot axis` are never moved.
- A bot only switches team while it is dead or spectating, never in the
  middle of a fight. In round-based gametypes like `sd` that usually means
  between rounds.

## Slots

Bots never take a private slot (`sv_privateClients`). `addbot` may fill
every public slot, but the minimum player floor always leaves one public
slot free, so a human can still join a full server and push a bot out.

The total player limit is `sv_maxclients`, up to 64 (the network protocol's
limit), and bots count toward it. Like on retail, `sv_maxclients` is latched,
so raise it on a running server like this:

```
rcon set sv_maxclients 32
rcon map_restart
```

The new limit takes effect at the restart or at the next map load. Players
stay connected through it.

## Names

A bot is named `[BOT] <name>`, drawn at random from a pool of 120 names in
`crates/server/src/botmgmt.rs`, such as `[BOT] Sir Camps-a-Lot`,
`[BOT] NadeSpammer` and `[BOT] PronePatrol`. No two connected bots share a
name. If more bots are connected than the pool has unused names, later ones
get a number, like `[BOT] NadeSpammer 2`. Every name fits retail's 31-byte
name limit with the prefix and a two-digit number.

The name draw uses its own random generator, so turning names on or off never
changes how a seeded server plays.

## Supported versions and gametypes

Bots play on 1.1, 1.5 and United Offensive servers, in every stock gametype:
`dm`, `tdm`, `sd`, `re` and `bel`, and UO's `ctf`, `dom`, `hq` and `bas`.
They are not very good. They find their way around stock maps, play each
gametype's objectives, and fight when `vc_bot_shoot` is on.
On objective gametypes a team spreads its bots over different routes,
reshuffles who guards what each round, sends all its S&D attackers to one
site and covers a planter, and in Retrieval escorts the carrier and holds
the goal ahead of him
([research/bot-objectives.md](research/bot-objectives.md) section 9). On a United
Offensive map with vehicles, a bot with no fight and no objective close by
walks to a vehicle near it and gets in with the use key, like a player. A
driver drives toward its objective or an enemy, waits a moment for a
teammate walking up to board, and in a tank stops to lay the turret on an
enemy and fire. A gunner fires the vehicle's machine gun. Bots get out when
they arrive, when stuck, or after 90 seconds.

## Known limits

- `status` shows a bot's address as `127.0.0.1:-1`.
- In UO, bots drive vehicles along the infantry graph over ground the hull
  fits, so a tank can still wedge itself; a driver backs off and gets out
  after four stalls. They never change seats, use the hatch or drive a
  vehicle at an enemy to run him over.
- Bot skill has no cvar; aim, reaction and memory are fixed in `bots.rs` (`Skill`).
