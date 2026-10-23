# Gameplay: powers, maluses and the punch

Blocks that burn down may drop a power. At most one item per block, decided at the moment the
block is destroyed. Walk onto an item to pick it up; bonuses last for the round, maluses for 10
seconds.

## Powers

Default chance is per destroyed block (`powerup_chance` 0.3 times the share of the kind).

| Kind | Icon (browser) | Terminal | Chance | Effect |
|------|----------------|----------|--------|--------|
| Extra bomb | blue `B` | `+B` | 6.0 % | one more bomb at a time (max 6) |
| Roller | green `R` | `+R` | 7.2 % | moves faster; the move cooldown drops by one tick (min 2) |
| Flame | orange `F` | `+F` | 7.2 % | blast range plus one (max 8) |
| Punch | purple `P` | `+P` | 0.9 % | lets you punch a bomb in front of you |
| Slow | blue `S` | `-S` | 3.0 % | malus: moves about a third slower for 10 s |
| Hyper speed | yellow `H` | `-H` | 2.4 % | malus: moves twice as fast for 10 s (hard to steer) |
| Small bomb | red `s` | `-s` | 3.3 % | malus: bombs placed in the next 10 s reach one square only |

Bonuses have a white frame in the browser and a green `+` in the terminal; maluses have a dark
red frame and a red `-`.

- A player has at most one malus. A new malus replaces the old one, the same malus again restarts
  its 10 seconds. A malus ends when the timer runs out, when the player is eliminated and at the
  start of a new round.
- Slow: the time between two moves is `ceil(cooldown * 4 / 3)` ticks. Hyper speed: `max(1,
  cooldown / 2)` ticks.
- Small bomb: only bombs placed while it is active have range 1; bombs already on the board keep
  theirs.
- Items stay on the board until somebody picks them up; flames do not destroy them.

## Punch

You need the punch power, which you keep for the rest of the round. Face a bomb on the next
square and press the punch key: the bomb jumps one square at a time (one hop every 2 ticks, 0.1 s)
in your facing direction until it lands.

- It lands on the first square with no wall, no block, no living player and no other bomb. Blocks,
  walls, players and other bombs are jumped over, not pushed.
- At the edge of the arena it lands on the nearest free square going back toward where it came
  from.
- The fuse keeps burning while it flies. A bomb whose fuse ends in the air explodes where it is.
- A flying bomb does not block anybody and flames do not set it off. After landing it behaves
  like any bomb, including chain reactions.
- Punching without the power, without a bomb in front of you or at a bomb that is already flying
  does nothing.
- Bots dodge flying bombs (they count the landing square and the current square as dangerous)
  but never punch.

## Controls

| Where | Move | Bomb | Punch |
|-------|------|------|-------|
| Browser | arrows or WASD | Space or E | Z or Q |
| Terminal P1 | arrow keys | Space | Z |
| Terminal P2 | W A S D | E | C |
| Terminal P3 | I J K L | O | U |
| Terminal P4 | T F G H | Y | N |

The terminal draws a bomb in flight as `(~` (cells are two characters wide) and lists the active
effects after each player's stats: `punch`, `slow 7s`, `hyper 4s`, `small 9s`. The browser lists
them in the player list next to the board and tints a player with a malus (blue slow, yellow
hyper speed, red small bomb).

## Forcing a power (developer mode)

Start the server with `--dev` (or `bomberust-cli host --dev`), join from the browser as the lobby
owner and use the DEV MODE panel: set the slider of a power to 100 % and the others to 0 % and
every block destroyed from then on drops that power. The "pause bots" button freezes the bots so
you can try a punch in peace. Other players cannot use the panel; without `--dev` there is none.
