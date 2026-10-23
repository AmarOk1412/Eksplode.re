# Audio: generated sound effects and music

All sound in the web page is **generated** by the page at run time with the Web Audio API,
from oscillators (sine, triangle, square, sawtooth) and filtered white noise. The game ships and
fetches no audio file, sample or recording, and no melody of anybody else is used: the two tunes
come from fixed patterns built by a small seeded generator in
[`crates/web/assets/audio.js`](../crates/web/assets/audio.js), so they are the same on every run
and free of third-party rights.

## Browser rules

- Browsers only allow sound after the person interacts with the page, so the audio context is
  created on the first key press, pointer press or touch. Before that nothing plays and nothing
  fails.
- If the browser has no Web Audio, or refuses, every audio call quietly does nothing and the game
  works as before.
- When the tab is hidden the audio is suspended (the music stops); it resumes when the tab is
  shown again.

## Settings

The speaker button on the join screen, the lobby and the match opens the sound settings: a music
slider, an effects slider, a mute toggle and a test-sound button. Changes apply within 50 ms
and are stored in the browser (`localStorage` key `bomberust.audio.v1`) as
`{"music":0.35,"effects":0.7,"muted":false}`, which are also the defaults. Levels are clamped
to 0..1. Three gain buses carry the sound: music and effects both feed a master bus that feeds the
speakers; the effective gain of a bus is zero while muted.

## Sound effects

Each snapshot of a match carries the events of its tick. Every event below makes at most one
sound per snapshot, and at most 8 sounds start per snapshot.

| Id | Trigger | Recipe (generated) |
|----|---------|--------------------|
| `click` | a button is pressed in the menu or lobby | 40 ms square tick at 1.2 kHz |
| `bombPlace` | `BombPlaced` | 80 ms triangle blip falling from 520 to 330 Hz |
| `explosion` | `BombExploded` | 600 ms white noise through a low-pass falling from 3000 to 120 Hz plus a 60 Hz sine thump; quieter when several go off in one tick |
| `bonus` | `PowerUpCollected` of a bonus | rising three-note arpeggio C5 E5 G5, 60 ms notes |
| `malus` | `PowerUpCollected` of a malus | 250 ms sawtooth buzz falling from 400 to 120 Hz |
| `punch` | `BombPunched` | 120 ms high-passed noise burst plus a triangle pitch drop from 700 to 200 Hz |
| `kick` | `BombKicked` | 110 ms triangle thud falling from 260 to 90 Hz |
| `land` | `BombLanded` | 90 ms sine thud at 140 Hz |
| `eliminated` | `PlayerEliminated` | four descending triangle notes, 100 ms each |
| `win` | `RoundEnded` with a winner | rising major fanfare C5 E5 G5 C6 |
| `draw` | `RoundEnded` without a winner | two low triangle tones, 196 Hz then 147 Hz |
| `warning` | `PatternStarted` (loud) and `BlockWarned` (soft) | 70 ms square beep falling from 880 to 660 Hz; announcement beeps are at most one per 250 ms |
| `impact` | `BlockLanded` | 200 ms sine thud falling from 90 to 45 Hz plus 180 ms low-passed noise falling from 900 to 90 Hz; louder when a player was eliminated in the same snapshot |

## Music

A lookahead step sequencer: a 25 ms timer schedules notes up to 120 ms ahead on the audio clock.
Tunes loop; switching tune fades the old one out and the new one in over 300 ms. The lobby tune
plays on the join screen and in the lobby, the match tune during a match; when a round ends the
match tune stops, the result sound plays and the lobby tune returns with the lobby.

| Name | When | Recipe (generated) |
|------|------|--------------------|
| `lobby` | join screen and lobby | 96 BPM, C major pentatonic arpeggio of soft triangle notes over a 4-bar triangle bass (C A G E), with a light noise tick on every beat; seed `0x10bb1` |
| `match` | during a match | 144 BPM, A natural minor, driving 8th-note sawtooth bass (A for one bar, F for the next), a 2-bar square-wave melody and a noise hi-hat on every 8th; seed `0x3a7c4` |
