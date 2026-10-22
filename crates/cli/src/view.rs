//! Rendering of a game to a ratatui frame.

use bomberust_core::{GameState, Outcome, Phase, PlayerId, Pos, PowerUpKind, Tile};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

pub const MIN_WIDTH: u16 = 40;
pub const MIN_HEIGHT: u16 = 20;
const STATUS_LINES: u16 = 4;

/// Extra information the view needs besides the game state.
#[derive(Clone, Debug, Default)]
pub struct ViewContext {
    pub paused: bool,
    /// Short description of each slot's controller; `None` for unused slots.
    pub tags: [Option<String>; 4],
}

/// Terminal size needed to draw `state`.
pub fn required_size(state: &GameState) -> (u16, u16) {
    let w = u16::from(state.arena.width()) * 2 + 2;
    let h = u16::from(state.arena.height()) + 2 + STATUS_LINES + 1;
    (w.max(MIN_WIDTH), h.max(MIN_HEIGHT))
}

/// Glyph and style of one tile. Priority: player, flame, bomb, power-up, terrain.
pub fn cell(state: &GameState, pos: Pos) -> (String, Style) {
    if let Some(p) = state.players.iter().find(|p| p.alive && p.pos == pos) {
        let style = Style::default()
            .fg(player_color(p.id))
            .add_modifier(Modifier::BOLD);
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
    if state.bombs.iter().any(|b| b.pos == pos) {
        return (
            "()".into(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );
    }
    if let Some(u) = state.powerups.iter().find(|u| u.pos == pos) {
        let glyph = match u.kind {
            PowerUpKind::ExtraBomb => "+B",
            PowerUpKind::Range => "+R",
            PowerUpKind::Speed => "+S",
        };
        return (glyph.into(), Style::default().fg(Color::Magenta));
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
pub fn render(frame: &mut Frame, state: &GameState, ctx: &ViewContext) {
    let area = frame.area();
    let (need_w, need_h) = required_size(state);
    if area.width < need_w || area.height < need_h {
        let msg = format!(
            "terminal too small (need {need_w}x{need_h}, have {}x{}); q quits",
            area.width, area.height
        );
        frame.render_widget(Paragraph::new(msg), area);
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

fn status_lines<'a>(state: &GameState, ctx: &ViewContext) -> Vec<Line<'a>> {
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
            format!(
                "{tag:<6} bombs {}/{} range {} speed {speed}",
                p.bombs_active, p.bombs_max, p.range
            )
        } else {
            format!("{tag:<6} eliminated")
        };
        lines.push(Line::from(vec![name, Span::raw(detail)]));
    }
    lines
}

fn message(state: &GameState, ctx: &ViewContext) -> Line<'static> {
    let text = match state.phase {
        Phase::Ended(Outcome::Winner(id)) => {
            format!("Player {} wins!  r: rematch  q: quit", id.index() + 1)
        }
        Phase::Ended(Outcome::Draw) => "Draw!  r: rematch  q: quit".to_string(),
        Phase::Playing if ctx.paused => "PAUSED  p: resume  q: quit".to_string(),
        Phase::Playing => "p: pause  q: quit".to_string(),
    };
    Line::from(Span::styled(
        text,
        Style::default().add_modifier(Modifier::BOLD),
    ))
}
