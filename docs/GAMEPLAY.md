# Gameplay: rooms, modes, powers, maluses, the punch and the kick

Blocks that burn down may drop a power. At most one item per block, decided at the moment the
block is destroyed. Walk onto an item to pick it up; bonuses last for the round, maluses for 10
seconds.

## Rooms and settings

A room is described by one set of settings that the owner (the first person who opens the room,
later the lowest slot held by a person) chooses before the match: a name of at most 24
characters, the mode, the board size, the round time and, for Team mode, friendly fire. The page
shows a creation menu on an empty server, the terminal host a setup screen; in the lobby everybody
sees the settings and only the owner changes them (page: the controls above the cards; terminal:
`m` mode, `s` size, `t` time, `f` friendly fire). Nobody can change them during a match, and they
stay for the rematch. The room closes when the last person leaves.

| Setting | Values | Default |
|---------|--------|---------|
| Board size | small 15 x 13, medium 19 x 15, large 25 x 19 (every size has the corner start areas, is fully reachable and fits the page; the terminal needs 40x20, 40x22 and 52x26) | small |
| Round time | 2, 3, 5 or 10 minutes | 3 |
| Friendly fire | on or off (Team mode only) | on |
| Mode | Normal, Team, Chaser | Normal |

## Normal mode and the end pattern

Everybody plays against everybody with the rules below. During the last 60 seconds of the round
time the board closes in: blocks fall in a clockwise spiral that starts at the outer ring and
ends in the middle, until at most one player is left. A falling block is announced on its square
for one second (the page draws a blinking red frame and a growing dark block, the terminal `!!`),
then it lands and the square is a wall that explosions cannot destroy. A player on the square is
eliminated, a bomb or a power on it is destroyed (a bomb does not explode, and its owner may place
it again), a bomb in the air is not touched. The pace is set so that every free square is covered
about a minute after the pattern starts, on every board size, so a Normal round always ends: with
the last player as the winner, or as a draw if the last players are eliminated in the same tick.
The round never ends by time while two players are alive. The page plays a warning sound and an
impact sound and shows "The walls close in!"; the clock then says `CLOSING`.

A round with a single participant (practice) has no end pattern and ends at the time limit.

## Team mode

Four participants (humans and bots) form two teams of two: slots 1 and 2 against slots 3 and 4. The
lobby cards, the status lines and the board show the teams (page: a blue or orange ring under the
characters and a team mark on the cards; terminal: `[A]` and `[B]`). The rules are those of Normal
mode, including the end pattern, but a team wins when at least one of its members is alive and
nobody of the other team is; if the last members of both teams are eliminated together the round is
a draw. The result names the winning team and its members.

Friendly fire is a room setting (on by default). With it on, explosions eliminate teammates like
anybody. With it off, an explosion that only teammates lit does not hurt a player, but a player is
still eliminated by their own bomb and by any explosion an opponent lit, and chain reactions still
work. A Team match cannot start with fewer than four participants; new rooms fill their empty
seats with bots, and an emptied seat takes a bot again with the lobby's "+" (`b` in the terminal). A bot that takes over for a player who left plays for the
same team and never targets a teammate; it also stays out of every explosion, including those of
its teammates, and does not place a bomb that would reach a teammate while friendly fire is on.

## Chaser mode

Four participants; one of them, drawn with the round's seed (so a rematch can give another one), is
the chaser and the other three are runners. The arena is nearly open: only about a third of the
fixed pillars (the squares where `(x/2 + y/2) % 3 == 0`) and a few blocks, and the middle and the
squares around it are free. The chaser starts in the middle with 2 bombs and 3 hearts; the runners
start in their corners (the chaser's own corner stays empty) with no bombs and cannot place one by
any means.

- A runner is eliminated by one explosion. The chaser loses one heart per explosion that reaches
  them, their own included, and is then protected for 2 seconds (the hearts blink in the page, `[]`
  shows in the terminal); with no heart left the chaser is eliminated.
- Bonuses appear by themselves, about every 5 seconds, on a random free square, with at most 6 on
  the board; blocks hide nothing. They are the roller (3 parts in 10), kick (2), punch (2) and the
  maluses slow, hyper speed and small bomb (1 each): runners can fight back with the punch and the
  kick (send the chaser's bombs back) but never get a bomb or a longer flame. The drop sliders of
  developer mode have no effect in this mode.
- The chaser wins when every runner is eliminated. The runners win when the chaser is eliminated or
  when the round time runs out with the chaser and a runner alive; if everybody is eliminated
  together it is a draw. There is no end pattern.
- The page marks the chaser with a crown and shows its hearts above it; the terminal marks it `*`
  with `<3 x2`. The results read "The chaser wins" and "The runners win".
- A bot chaser hunts the runners and plants bombs when it has a way out; a bot runner keeps away
  from the chaser (when it comes within five squares), collects bonuses, flees every explosion and
  sends a bomb in front of it away when it has the punch.

## Powers

Default chance is per destroyed block (`powerup_chance` 0.3 times the share of the kind).

| Kind | Icon (browser) | Terminal | Chance | Effect |
|------|----------------|----------|--------|--------|
| Extra bomb | a bomb with a plus | `+B` | 5.7 % | one more bomb at a time (max 6) |
| Roller | a rollerskate | `+R` | 6.6 % | moves faster; the move cooldown drops by one tick (min 2) |
| Flame | a flame | `+F` | 6.6 % | blast range plus one (max 8) |
| Punch | a boxing glove | `+P` | 0.9 % | lets you punch a bomb in front of you |
| Kick | a boot | `+K` | 1.5 % | walking into a bomb makes it slide until the next square is blocked |
| Slow | a snail | `-S` | 3.0 % | malus: moves about a third slower for 10 s |
| Hyper speed | a lightning bolt | `-H` | 2.4 % | malus: moves twice as fast for 10 s (hard to steer) |
| Small bomb | a tiny bomb with a minus | `-s` | 3.3 % | malus: bombs placed in the next 10 s reach one square only |

The icons are small pixel pictures (10 by 10 pixels, drawn by the render module); on a very small
board, where a picture would be unreadable, the browser shows the letter of the power instead.
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

## Kick

A player with the kick pushes a bomb by walking into it. The bomb slides away from the player, one
square every 3 ticks (0.15 s), and stops on the last square before the next blocked one: a block,
a wall, another bomb, a living player or the edge of the arena.

- If the square behind the bomb is already blocked, nothing moves and the bomb still blocks you.
- The fuse keeps burning while the bomb slides; flames set it off like any bomb, and it explodes
  where it is when the fuse ends (possibly in the middle of the slide).
- A sliding bomb blocks everybody. A player who steps into its path stops it on the square before
  them.
- Only landed bombs can be kicked, not bombs flying after a punch. Punching a sliding bomb next to
  you ends the slide and sends it flying.
- The kick is kept for the round. Bots do not kick on purpose; they treat both the square where the
  fuse ends and the stop square of a sliding bomb as dangerous.
- The terminal draws a sliding bomb as `(>`, `(<`, `(^` or `(v` by its direction and lists `kick`
  in the status line. In the browser the bomb rolls from square to square.

The kick is the "push the bomb" bonus; it was requested as a "roller", a name the game keeps for
the speed bonus.

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
effects after each player's stats: `punch`, `kick`, `slow 7s`, `hyper 4s`, `small 9s`. The browser lists
them in the player list next to the board and tints a player with a malus (blue slow, yellow
hyper speed, red small bomb).

## Characters

In a match each player is drawn from the side they face: front when moving down or standing at the
start of a round, back when moving up, side when moving left or right (the right side is the left
side mirrored). The same colour, accessories and costume are drawn in every view (the body
silhouette alternates with the slot and is not a choice); from behind
there is no face, from the side there is one eye and the hat brim points ahead. While a player
walks the sprite alternates two steps (feet and arms swap, one pixel of bob) at the pace of their
steps, so hyper speed walks faster and slow walks slower; a standing player shows an idle pose in
the last direction faced.

## Forcing a power (developer mode)

Start the server with `--dev` (or `eksplode-re-cli host --dev`), join from the browser as the lobby
owner and use the DEV MODE panel: set the slider of a power to 100 % and the others to 0 % and
every block destroyed from then on drops that power. The "pause bots" button freezes the bots so
you can try a punch in peace. Other players cannot use the panel; without `--dev` there is none.
