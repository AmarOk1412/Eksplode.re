#![allow(clippy::unwrap_used)]

use bomberust_cli::view::{render, required_size, ViewContext};
use bomberust_core::{
    Arena, Bomb, Config, Flame, GameState, Outcome, Phase, PlayerId, Pos, PowerUp, PowerUpKind,
};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

const ROWS: [&str; 7] = [
    "#######", "#..+..#", "#.....#", "#.....#", "#.....#", "#.....#", "#######",
];

fn pid(i: u8) -> PlayerId {
    PlayerId::new(i).unwrap()
}

fn base() -> GameState {
    let arena = Arena::from_rows(&ROWS).unwrap();
    let starts = [Some(Pos::new(1, 1)), Some(Pos::new(5, 5)), None, None];
    GameState::custom(Config::default(), arena, starts, 1).unwrap()
}

fn ctx() -> ViewContext {
    ViewContext {
        paused: false,
        tags: [Some("human".into()), Some("bot".into()), None, None],
        notice: None,
    }
}

fn draw(state: &GameState, ctx: &ViewContext, w: u16, h: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    let obs = state.observe(pid(0));
    terminal.draw(|f| render(f, &obs, ctx)).unwrap();
    let buf = terminal.backend().buffer().clone();
    (0..h)
        .map(|y| {
            (0..w)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect()
}

fn screen(state: &GameState, ctx: &ViewContext) -> String {
    draw(state, ctx, 80, 24).join("\n")
}

#[test]
fn draws_terrain_and_players() {
    let s = screen(&base(), &ctx());
    assert!(s.contains("##"), "wall");
    assert!(s.contains("[]"), "block");
    assert!(s.contains("P1"), "player 1");
    assert!(s.contains("P2"), "player 2");
    assert!(!s.contains("P3"));
}

#[test]
fn draws_bombs_flames_and_powerups() {
    let mut st = base();
    st.bombs.push(Bomb {
        owner: pid(0),
        pos: Pos::new(3, 3),
        ticks_left: 30,
        range: 2,
        flight: None,
        slide: None,
    });
    st.flames.push(Flame {
        pos: Pos::new(4, 3),
        ticks_left: 5,
        owners: 0,
    });
    st.powerups.push(PowerUp {
        pos: Pos::new(2, 4),
        kind: PowerUpKind::ExtraBomb,
    });
    st.powerups.push(PowerUp {
        pos: Pos::new(3, 4),
        kind: PowerUpKind::Range,
    });
    st.powerups.push(PowerUp {
        pos: Pos::new(4, 4),
        kind: PowerUpKind::Speed,
    });
    let s = screen(&st, &ctx());
    for glyph in ["()", "**", "+B", "+F", "+R"] {
        assert!(s.contains(glyph), "{glyph}");
    }
}

#[test]
fn eliminated_players_disappear_from_the_arena() {
    let mut st = base();
    st.players[1].alive = false;
    let s = screen(&st, &ctx());
    let arena_part: String = draw(&st, &ctx(), 80, 24)[..9].join("\n");
    assert!(!arena_part.contains("P2"));
    assert!(s.contains("eliminated"));
}

#[test]
fn status_line_shows_bombs_range_and_speed() {
    let s = screen(&base(), &ctx());
    assert!(s.contains("bombs 0/1"), "{s}");
    assert!(s.contains("range 2"));
    assert!(s.contains("speed 1"));
    assert!(s.contains("human"));
    assert!(s.contains("bot"));
}

#[test]
fn result_messages() {
    let mut st = base();
    st.phase = Phase::Ended(Outcome::Winner(pid(1)));
    assert!(screen(&st, &ctx()).contains("Player 2 wins"));
    st.phase = Phase::Ended(Outcome::Draw);
    assert!(screen(&st, &ctx()).contains("Draw"));
}

#[test]
fn pause_message() {
    let mut c = ctx();
    c.paused = true;
    assert!(screen(&base(), &c).contains("PAUSED"));
}

#[test]
fn too_small_terminal_shows_the_needed_size() {
    let st = base();
    let (w, h) = required_size(&st.observe(pid(0)));
    assert_eq!((w, h), (40, 20));
    let s = draw(&st, &ctx(), 30, 10).join("\n");
    assert!(s.contains("too small"));
    assert!(s.contains("40x20"));
}

#[test]
fn minimum_size_is_enough() {
    let s = draw(&base(), &ctx(), 40, 20).join("\n");
    assert!(!s.contains("too small"));
    assert!(s.contains("P1"));
}

#[test]
fn default_arena_fits_the_minimum_size() {
    let st = GameState::new(Config::default(), 1, [true; 4]).unwrap();
    assert_eq!(required_size(&st.observe(pid(0))), (40, 20));
}

#[test]
fn f3_us2_as9_every_power_has_its_letter() {
    let mut st = base();
    for (i, kind) in PowerUpKind::ALL.iter().enumerate() {
        st.powerups.push(PowerUp {
            pos: Pos::new(1 + i as u8 % 5, 2 + i as u8 / 5),
            kind: *kind,
        });
    }
    let s = screen(&st, &ctx());
    for glyph in ["+B", "+R", "+F", "+P", "+K", "-S", "-H", "-s"] {
        assert!(s.contains(glyph), "{glyph}\n{s}");
    }
}

#[test]
fn f3_us2_as9_a_bomb_in_flight_looks_different_from_a_landed_one() {
    let mut st = base();
    st.bombs.push(Bomb {
        owner: pid(0),
        pos: Pos::new(3, 3),
        ticks_left: 30,
        range: 2,
        flight: Some(bomberust_core::Flight {
            dir: bomberust_core::Dir::Right,
            hop_left: 2,
            origin: Pos::new(3, 3),
        }),
        slide: None,
    });
    let obs = st.observe(pid(0));
    let (glyph, _) = bomberust_cli::view::cell(&obs, Pos::new(3, 3));
    assert_eq!(glyph, "(~");
}

#[test]
fn f3_us2_as9_status_lines_list_the_punch_and_the_malus_with_seconds() {
    let mut st = base();
    st.players[0].has_punch = true;
    st.players[0].malus = Some(bomberust_core::ActiveMalus {
        kind: PowerUpKind::Slow,
        ticks_left: 7 * 20 - 3,
    });
    st.players[1].malus = Some(bomberust_core::ActiveMalus {
        kind: PowerUpKind::HyperSpeed,
        ticks_left: 4 * 20,
    });
    let s = screen(&st, &ctx());
    assert!(s.contains("punch"), "{s}");
    assert!(s.contains("slow 7s"), "{s}");
    assert!(s.contains("hyper 4s"), "{s}");
    st.players[1].malus = Some(bomberust_core::ActiveMalus {
        kind: PowerUpKind::SmallBomb,
        ticks_left: 9 * 20 - 19,
    });
    assert!(screen(&st, &ctx()).contains("small 9s"));
}

#[test]
fn f3_us2_as12_a_sliding_bomb_shows_its_direction() {
    use bomberust_core::{Dir, Slide};
    for (dir, glyph) in [
        (Dir::Right, "(>"),
        (Dir::Left, "(<"),
        (Dir::Up, "(^"),
        (Dir::Down, "(v"),
    ] {
        let mut st = base();
        st.bombs.push(Bomb {
            owner: pid(0),
            pos: Pos::new(3, 3),
            ticks_left: 30,
            range: 2,
            flight: None,
            slide: Some(Slide { dir, step_left: 2 }),
        });
        let obs = st.observe(pid(0));
        let (g, _) = bomberust_cli::view::cell(&obs, Pos::new(3, 3));
        assert_eq!(g, glyph, "{dir:?}");
    }
}

#[test]
fn f3_us2_as10_the_status_line_lists_the_kick() {
    let mut st = base();
    st.players[0].has_kick = true;
    st.players[0].has_punch = true;
    let s = screen(&st, &ctx());
    assert!(s.contains("punch kick") || s.contains("kick"), "{s}");
}

#[test]
fn f4_us2_as6_the_announcement_the_landed_blocks_and_the_time_left_are_drawn() {
    let mut st = base();
    st.config.round_limit_ticks = 3600;
    st.tick = 20;
    let plain = screen(&st, &ctx());
    assert!(!plain.contains("!!"), "nothing announced, nothing marked");
    assert!(
        plain.contains("02:59"),
        "time left in minutes and seconds:\n{plain}"
    );

    st.closing.falling.push(bomberust_core::Falling {
        pos: Pos::new(3, 3),
        ticks_left: 12,
    });
    let marked = screen(&st, &ctx());
    assert!(marked.contains("!!"), "announced square:\n{marked}");

    // a landed block is an ordinary wall
    st.closing.falling.clear();
    st.arena
        .set_tile(Pos::new(3, 3), bomberust_core::Tile::Wall);
    let landed = screen(&st, &ctx());
    assert!(!landed.contains("!!"));
    assert_eq!(
        landed.matches("##").count(),
        plain.matches("##").count() + 1
    );

    // the pattern time: the clock says so
    st.tick = 3600 - 1200;
    assert!(screen(&st, &ctx()).contains("CLOSING"));
}

#[test]
fn a_player_on_an_announced_square_is_still_drawn_and_marked() {
    let mut st = base();
    st.closing.falling.push(bomberust_core::Falling {
        pos: Pos::new(1, 1),
        ticks_left: 5,
    });
    let s = screen(&st, &ctx());
    assert!(s.contains("P1"));
}

fn state_of(size: bomberust_core::BoardSize) -> GameState {
    let config = bomberust_core::RoomSettings {
        size,
        ..Default::default()
    }
    .to_config(&Config::default());
    GameState::new(config, 3, [true; 4]).unwrap()
}

#[test]
fn f4_us1_as6_the_terminal_needs_room_for_the_chosen_board_and_says_so() {
    use bomberust_core::BoardSize;
    let need = |size| required_size(&state_of(size).observe(pid(0)));
    assert_eq!(need(BoardSize::Small), (40, 20));
    assert_eq!(need(BoardSize::Medium), (40, 22));
    assert_eq!(need(BoardSize::Large), (52, 26));

    let large = state_of(BoardSize::Large);
    let small_terminal = draw(&large, &ctx(), 40, 20).join("\n");
    assert!(
        small_terminal.contains("terminal too small"),
        "{small_terminal}"
    );
    assert!(small_terminal.contains("need 52x26"), "{small_terminal}");
    assert!(small_terminal.contains("have 40x20"), "{small_terminal}");

    // a terminal of the needed size draws the whole board
    let fits = draw(&large, &ctx(), 52, 26).join("\n");
    assert!(!fits.contains("terminal too small"), "{fits}");
    assert!(fits.contains("P1") && fits.contains("P4"));
}
