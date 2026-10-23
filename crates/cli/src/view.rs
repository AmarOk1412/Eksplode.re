//! Rendering of a game to a ratatui frame.

use bomberust_core::{Dir, Observation, Outcome, Phase, PlayerId, Pos, PowerUpKind, Tile};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

pub const MIN_WIDTH: u16 = 40;
pub const MIN_HEIGHT: u16 = 20;
const STATUS_LINES: u16 = 4;

/// Extra information the view needs besides the observation.
#[derive(Clone, Debug, Default)]
pub struct ViewContext {
    pub paused: bool,
    /// Short description of each slot's controller; `None` for unused slots.
    pub tags: [Option<String>; 4],
    /// Replaces the hint line, e.g. a network message.
    pub notice: Option<String>,
}

/// Terminal size needed to draw the observation.
pub fn required_size(state: &Observation) -> (u16, u16) {
    let w = u16::from(state.arena.width()) * 2 + 2;
    let h = u16::from(state.arena.height()) + 2 + STATUS_LINES + 1;
    (w.max(MIN_WIDTH), h.max(MIN_HEIGHT))
}

/// Glyph and style of one tile. Priority: player, flame, bomb, power-up, terrain.
pub fn cell(state: &Observation, pos: Pos) -> (String, Style) {
    let announced = state.falling.iter().any(|f| f.pos == pos);
    if let Some(p) = state.players.iter().find(|p| p.alive && p.pos == pos) {
        let mut style = Style::default()
            .fg(player_color(p.id))
            .add_modifier(Modifier::BOLD);
        if announced {
            // a block is about to land on this player
            style = style.add_modifier(Modifier::REVERSED);
        }
        if p.role == bomberust_core::Role::Chaser {
            style = style.bg(Color::Yellow).fg(Color::Black);
        }
        return (format!("P{}", p.id.index() + 1), style);
    }
    if state.flames.iter().any(|f| f.pos == pos) {
        return (
            "**".into(),
            Style::default()
                .fg(Color::LightRed)
                .add_modifier(Modifier::BOLD),
        );
    }
    if let Some(b) = state.bombs.iter().find(|b| b.pos == pos) {
        // a bomb in the air after a punch trails a wave
        let glyph = match (b.flight, b.slide) {
            (Some(_), _) => "(~",
            (None, Some(s)) => match s.dir {
                Dir::Right => "(>",
                Dir::Left => "(<",
                Dir::Up => "(^",
                Dir::Down => "(v",
            },
            (None, None) => "()",
        };
        return (
            glyph.into(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );
    }
    if let Some(u) = state.powerups.iter().find(|u| u.pos == pos) {
        // bonuses are green with a plus, maluses red with a minus
        let glyph = match u.kind {
            PowerUpKind::ExtraBomb => "+B",
            PowerUpKind::Speed => "+R",
            PowerUpKind::Range => "+F",
            PowerUpKind::Punch => "+P",
            PowerUpKind::Kick => "+K",
            PowerUpKind::Slow => "-S",
            PowerUpKind::HyperSpeed => "-H",
            PowerUpKind::SmallBomb => "-s",
        };
        let colour = if u.kind.is_malus() {
            Color::Red
        } else {
            Color::Green
        };
        return (glyph.into(), Style::default().fg(colour));
    }
    if announced {
        // the end pattern is about to cover this square
        return (
            "!!".into(),
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD | Modifier::SLOW_BLINK),
        );
    }
    match state.arena.tile(pos) {
        Tile::Wall => ("##".into(), Style::default().fg(Color::DarkGray)),
        Tile::Block => ("[]".into(), Style::default().fg(Color::Indexed(130))),
        Tile::Floor => ("  ".into(), Style::default()),
    }
}

pub fn player_color(id: PlayerId) -> Color {
    match id.index() {
        0 => Color::Red,
        1 => Color::LightBlue,
        2 => Color::Green,
        _ => Color::Yellow,
    }
}

/// Draws the arena, one status line per participant and a message line.
pub fn render(frame: &mut Frame, state: &Observation, ctx: &ViewContext) {
    let area = frame.area();
    let (need_w, need_h) = required_size(state);
    if area.width < need_w || area.height < need_h {
        // one short line each, so even the narrowest terminal shows all of it
        let lines = vec![
            Line::from("terminal too small"),
            Line::from(format!("need {need_w}x{need_h}")),
            Line::from(format!("have {}x{}", area.width, area.height)),
            Line::from("q quits"),
        ];
        frame.render_widget(Paragraph::new(lines), area);
        return;
    }

    let arena_w = u16::from(state.arena.width()) * 2 + 2;
    let arena_h = u16::from(state.arena.height()) + 2;
    let x = area.x + (area.width - arena_w) / 2;
    let arena_rect = Rect::new(x, area.y, arena_w, arena_h);
    let lines: Vec<Line> = (0..state.arena.height())
        .map(|y| {
            let spans: Vec<Span> = (0..state.arena.width())
                .map(|x| {
                    let (text, style) = cell(state, Pos::new(x, y));
                    Span::styled(text, style)
                })
                .collect();
            Line::from(spans)
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" Bomberust ")),
        arena_rect,
    );

    let status_rect = Rect::new(area.x, area.y + arena_h, area.width, STATUS_LINES);
    frame.render_widget(Paragraph::new(status_lines(state, ctx)), status_rect);

    let msg_rect = Rect::new(area.x, area.y + arena_h + STATUS_LINES, area.width, 1);
    frame.render_widget(Paragraph::new(message(state, ctx)), msg_rect);
}

/// The time left, or `CLOSING` once the end pattern has started and the round ends when one is
/// left, however long it takes.
pub fn clock(state: &Observation) -> String {
    let c = &state.config;
    let pattern = matches!(
        c.mode,
        bomberust_core::Mode::Normal | bomberust_core::Mode::Team
    ) && c.closing_ticks > 0
        && c.closing_ticks < c.round_limit_ticks;
    if pattern && state.tick >= c.round_limit_ticks - c.closing_ticks {
        return "CLOSING".to_string();
    }
    let left = c
        .round_limit_ticks
        .saturating_sub(state.tick)
        .div_ceil(c.tick_hz.max(1));
    format!("{:02}:{:02}", left / 60, left % 60)
}

fn status_lines<'a>(state: &Observation, ctx: &ViewContext) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    for p in &state.players {
        let Some(tag) = &ctx.tags[p.id.index()] else {
            continue;
        };
        let name = Span::styled(
            format!("P{} ", p.id.index() + 1),
            Style::default()
                .fg(player_color(p.id))
                .add_modifier(Modifier::BOLD),
        );
        let detail = if p.alive {
            let speed = state.config.base_move_cooldown + 1 - p.move_cooldown;
            let mut effects = String::new();
            if p.has_punch {
                effects.push_str(" punch");
            }
            if p.has_kick {
                effects.push_str(" kick");
            }
            if let Some(m) = p.malus {
                let name = match m.kind {
                    PowerUpKind::Slow => "slow",
                    PowerUpKind::HyperSpeed => "hyper",
                    _ => "small",
                };
                let seconds = m.ticks_left.div_ceil(state.config.tick_hz.max(1));
                effects.push_str(&format!(" {name} {seconds}s"));
            }
            let role = match p.role {
                bomberust_core::Role::Chaser => {
                    let shield = if p.shield_left > 0 { " []" } else { "" };
                    format!(" <3 x{}{shield}", p.hearts)
                }
                bomberust_core::Role::Runner => " runner".to_string(),
                bomberust_core::Role::Normal => String::new(),
            };
            format!(
                "{tag:<6} bombs {}/{} range {} speed {speed}{effects}{role}",
                p.bombs_active, p.bombs_max, p.range
            )
        } else {
            format!("{tag:<6} eliminated")
        };
        let mut spans = vec![name];
        if p.role == bomberust_core::Role::Chaser {
            spans.push(Span::styled(
                "* ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        if let Some(t) = p.team {
            spans.push(Span::styled(
                format!("[{}] ", team_letter(t)),
                Style::default().add_modifier(Modifier::BOLD),
            ));
        }
        spans.push(Span::raw(detail));
        lines.push(Line::from(spans));
    }
    lines
}

/// `A` for team 0 and `B` for team 1.
pub fn team_letter(team: u8) -> char {
    if team == 0 {
        'A'
    } else {
        'B'
    }
}

/// `P1, P2`: the members of a team.
pub fn team_members(state: &Observation, team: u8) -> String {
    state
        .players
        .iter()
        .filter(|p| p.team == Some(team))
        .map(|p| format!("P{}", p.id.index() + 1))
        .collect::<Vec<_>>()
        .join(", ")
}

fn message(state: &Observation, ctx: &ViewContext) -> Line<'static> {
    let text = match (&ctx.notice, state.phase) {
        (Some(n), _) => n.clone(),
        (None, Phase::Ended(Outcome::Winner(id))) => {
            format!("Player {} wins!  r: rematch  q: quit", id.index() + 1)
        }
        (None, Phase::Ended(Outcome::Draw)) => "Draw!  r: rematch  q: quit".to_string(),
        (None, Phase::Ended(Outcome::TeamWin(t))) => {
            format!(
                "Team {} wins ({})!  r: rematch  q: quit",
                team_letter(t),
                team_members(state, t)
            )
        }
        (None, Phase::Ended(Outcome::ChaserWins)) => {
            "The chaser wins!  r: rematch  q: quit".to_string()
        }
        (None, Phase::Ended(Outcome::RunnersWin)) => {
            "The runners win!  r: rematch  q: quit".to_string()
        }
        (None, Phase::Playing) if ctx.paused => "PAUSED  p: resume  q: quit".to_string(),
        (None, Phase::Playing) => format!("{}  p: pause  q: quit", clock(state)),
    };
    Line::from(Span::styled(
        text,
        Style::default().add_modifier(Modifier::BOLD),
    ))
}
