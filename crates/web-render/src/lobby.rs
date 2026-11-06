//! Deterministic decorative scenes, independent of match observations and rules.

use eksplode_re_core::Rng;

use crate::commands::{Command, Kind};
use crate::look::{default_for, runs_view, Frame, Look, View, GRID_ROWS};
use crate::sprites::{bomb_colour, runs, BOMB};

pub const DURATION: u16 = 300;
pub const WIDTH: f32 = 640.0;
pub const HEIGHT: f32 = 200.0;

pub mod motif {
    pub const RUN: u16 = 1;
    pub const DROP: u16 = 1 << 1;
    pub const BLAST: u16 = 1 << 2;
    pub const SAFE_EXIT: u16 = 1 << 3;
    pub const SAME_DIRECTION: u16 = 1 << 4;
    pub const OPPOSITE_DIRECTION: u16 = 1 << 5;
    pub const BACKGROUND: u16 = 1 << 6;
    pub const ELIMINATION: u16 = 1 << 7;
    pub const PUSH: u16 = 1 << 8;
    pub const FLATTEN: u16 = 1 << 9;
    pub const KART_LEFT: u16 = 1 << 10;
    pub const KART_RIGHT: u16 = 1 << 11;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VignetteKind {
    RunDrop,
    SameLevel,
    CounterRunBlast,
    BackgroundRun,
    KartLeft,
    KartRight,
    PushSquash,
}

pub const ALL_KINDS: [VignetteKind; 7] = [
    VignetteKind::RunDrop,
    VignetteKind::SameLevel,
    VignetteKind::CounterRunBlast,
    VignetteKind::BackgroundRun,
    VignetteKind::KartLeft,
    VignetteKind::KartRight,
    VignetteKind::PushSquash,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    Background,
    Foreground,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pose {
    Idle,
    StepA,
    StepB,
    Push,
    Ride,
    Squash,
    Gone,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecorativeActor {
    pub identity: u8,
    pub look: Look,
    pub direction: Direction,
    pub depth: Depth,
    pub position: (f32, f32),
    pub pose: Pose,
    pub scale: (f32, f32),
    pub opacity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BombRole {
    Dropped,
    Pushed,
    Incoming,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BombState {
    Absent,
    Fuse,
    Moving,
    Blast,
    Cleared,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecorativeBomb {
    pub identity: u8,
    pub role: BombRole,
    pub position: (f32, f32),
    pub state: BombState,
    pub progress: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectKind {
    LocalBlast,
    Smoke,
    Flatten,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecorativeEffect {
    pub kind: EffectKind,
    pub position: (f32, f32),
    pub size: f32,
    pub opacity: f32,
    pub start: u16,
    pub end: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneFrame {
    pub kind: VignetteKind,
    pub phase: f32,
    pub actors: Vec<DecorativeActor>,
    pub bombs: Vec<DecorativeBomb>,
    pub effects: Vec<DecorativeEffect>,
    pub coverage: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneError {
    InvalidTicks,
    InvalidAlpha,
}

pub struct LobbyTimeline {
    rng: Rng,
    deck: [VignetteKind; 7],
    cursor: usize,
    local_tick: u16,
    previous_kind: Option<VignetteKind>,
}

impl LobbyTimeline {
    pub fn new(seed: u32) -> Self {
        let mut scene = Self {
            rng: Rng::new(u64::from(seed)),
            deck: ALL_KINDS,
            cursor: 0,
            local_tick: 0,
            previous_kind: None,
        };
        scene.shuffle();
        scene
    }

    fn shuffle(&mut self) {
        self.deck = ALL_KINDS;
        for index in (1..self.deck.len()).rev() {
            let other = self.rng.next_below((index + 1) as u32) as usize;
            self.deck.swap(index, other);
        }
        if self.previous_kind == Some(self.deck[0]) {
            self.deck.swap(0, 1);
        }
    }

    pub fn deck(&self) -> [VignetteKind; 7] {
        self.deck
    }
    pub fn cursor(&self) -> usize {
        self.cursor
    }
    pub fn kind(&self) -> VignetteKind {
        self.deck[self.cursor]
    }
    pub fn local_tick(&self) -> u16 {
        self.local_tick
    }

    pub fn advance(&mut self, ticks: u32) -> Result<(), SceneError> {
        if ticks > 15 {
            return Err(SceneError::InvalidTicks);
        }
        self.local_tick += ticks as u16;
        if self.local_tick >= DURATION {
            self.local_tick -= DURATION;
            self.previous_kind = Some(self.kind());
            self.cursor += 1;
            if self.cursor == self.deck.len() {
                self.cursor = 0;
                self.shuffle();
            }
        }
        Ok(())
    }

    pub fn sample(&self, alpha: f32) -> Result<SceneFrame, SceneError> {
        if !alpha.is_finite() || !(0.0..1.0).contains(&alpha) {
            return Err(SceneError::InvalidAlpha);
        }
        let mut frame = SceneFrame {
            kind: self.kind(),
            phase: f32::from(self.local_tick) + alpha,
            actors: Vec::new(),
            bombs: Vec::new(),
            effects: Vec::new(),
            coverage: 0,
        };
        match frame.kind {
            VignetteKind::RunDrop => run_drop(&mut frame),
            VignetteKind::SameLevel => same_level(&mut frame),
            VignetteKind::CounterRunBlast => counter_run(&mut frame),
            VignetteKind::BackgroundRun => background_run(&mut frame),
            VignetteKind::PushSquash => push_squash(&mut frame),
            VignetteKind::KartLeft => kart_pass(&mut frame, Direction::Left),
            VignetteKind::KartRight => kart_pass(&mut frame, Direction::Right),
        }
        frame.actors.sort_by_key(|actor| {
            if actor.depth == Depth::Background {
                0
            } else {
                1
            }
        });
        Ok(frame)
    }
}

fn runner(
    identity: u8,
    position: f32,
    direction: Direction,
    depth: Depth,
    phase: f32,
) -> DecorativeActor {
    let background = depth == Depth::Background;
    let scale = if background { 2.0 } else { 3.4 };
    DecorativeActor {
        identity,
        look: default_for(usize::from(identity)),
        direction,
        depth,
        position: (position, if background { 124.0 } else { 174.0 }),
        pose: if (phase / 7.0) as u16 % 2 == 0 {
            Pose::StepA
        } else {
            Pose::StepB
        },
        scale: (scale, scale),
        opacity: 1.0,
    }
}

fn run_drop(frame: &mut SceneFrame) {
    let phase = frame.phase;
    let position = 20.0 + phase * 2.7;
    if position < WIDTH + 24.0 {
        frame.actors.push(runner(
            0,
            position,
            Direction::Right,
            Depth::Foreground,
            phase,
        ));
        frame.coverage |= motif::RUN;
        if position >= WIDTH - 24.0 {
            frame.coverage |= motif::SAFE_EXIT;
        }
    }
    if (80.0..220.0).contains(&phase) {
        let exploding = phase >= 185.0;
        frame.bombs.push(DecorativeBomb {
            identity: 0,
            role: BombRole::Dropped,
            position: (236.0, 174.0),
            state: if exploding {
                BombState::Blast
            } else {
                BombState::Fuse
            },
            progress: ((phase - 80.0) / 105.0).min(1.0),
        });
        if exploding {
            let progress = (phase - 185.0) / 35.0;
            frame.effects.push(DecorativeEffect {
                kind: EffectKind::LocalBlast,
                position: (236.0, 159.0),
                size: 28.0 + 16.0 * (progress * std::f32::consts::PI).sin(),
                opacity: 1.0 - progress,
                start: 185,
                end: 220,
            });
            frame.coverage |= motif::BLAST;
        } else {
            frame.coverage |= motif::DROP;
        }
    }
}

fn same_level(frame: &mut SceneFrame) {
    run_drop(frame);
    let position = frame.phase * 2.7 - 80.0;
    if (-24.0..WIDTH + 24.0).contains(&position) {
        frame.actors.push(runner(
            2,
            position,
            Direction::Right,
            Depth::Foreground,
            frame.phase,
        ));
        if frame.actors.len() == 2 && position > 0.0 {
            frame.coverage |= motif::SAME_DIRECTION;
        }
    }
}

fn background_run(frame: &mut SceneFrame) {
    run_drop(frame);
    for (identity, position, direction) in [
        (2, frame.phase * 2.5 - 50.0, Direction::Right),
        (3, 620.0 - frame.phase * 2.4, Direction::Left),
    ] {
        if (-24.0..WIDTH + 24.0).contains(&position) {
            frame.actors.push(runner(
                identity,
                position,
                direction,
                Depth::Background,
                frame.phase,
            ));
            if (0.0..WIDTH).contains(&position) {
                frame.coverage |= motif::BACKGROUND;
            }
        }
    }
}

fn counter_run(frame: &mut SceneFrame) {
    run_drop(frame);
    frame.actors.clear();
    frame.coverage &= !(motif::RUN | motif::SAFE_EXIT);
    let phase = frame.phase;
    if phase < 235.0 {
        let mut lead = runner(
            0,
            20.0 + phase.min(80.0) * 2.7,
            Direction::Right,
            Depth::Foreground,
            phase,
        );
        if phase >= 185.0 {
            let fade = (phase - 185.0) / 50.0;
            lead.pose = Pose::Squash;
            lead.scale = (3.4 + fade * 2.0, 3.4 * (1.0 - fade * 0.9));
            lead.opacity = 1.0 - fade;
            frame.coverage |= motif::ELIMINATION;
        } else if phase >= 80.0 {
            lead.pose = Pose::Idle;
        }
        frame.actors.push(lead);
    }
    let position = 630.0 - phase * 2.7;
    if position > -24.0 {
        frame.actors.push(runner(
            2,
            position,
            Direction::Left,
            Depth::Foreground,
            phase,
        ));
        frame.coverage |= motif::RUN;
    }
    if phase < 80.0 && frame.actors.len() == 2 {
        frame.coverage |= motif::OPPOSITE_DIRECTION;
    }
}

fn push_squash(frame: &mut SceneFrame) {
    let phase = frame.phase;
    let position = 20.0 + phase.min(170.0) * 1.4;
    if phase < 230.0 {
        let mut actor = runner(0, position, Direction::Right, Depth::Foreground, phase);
        actor.pose = Pose::Push;
        if phase >= 170.0 {
            let fade = (phase - 170.0) / 60.0;
            actor.pose = Pose::Squash;
            actor.scale = (4.8, 3.4 * (1.0 - 0.9 * (fade * 3.0).min(1.0)));
            actor.opacity = 1.0 - fade;
            frame.effects.push(DecorativeEffect {
                kind: EffectKind::Flatten,
                position: (position, 169.0),
                size: 15.0 + fade * 20.0,
                opacity: (1.0 - fade) * 0.6,
                start: 170,
                end: 230,
            });
            frame.coverage |= motif::FLATTEN;
        } else {
            frame.coverage |= motif::PUSH;
        }
        frame.actors.push(actor);
    }
    if phase < 210.0 {
        frame.bombs.push(DecorativeBomb {
            identity: 0,
            role: BombRole::Pushed,
            position: (position + 34.0, 174.0),
            state: BombState::Moving,
            progress: 0.0,
        });
    }
    if (110.0..218.0).contains(&phase) {
        frame.bombs.push(DecorativeBomb {
            identity: 1,
            role: BombRole::Incoming,
            position: (680.0 - (phase - 100.0) * 6.0, 174.0),
            state: BombState::Moving,
            progress: 0.0,
        });
    }
}

fn kart_pass(frame: &mut SceneFrame, direction: Direction) {
    let position = if direction == Direction::Right {
        -36.0 + frame.phase * 2.7
    } else {
        WIDTH + 36.0 - frame.phase * 2.7
    };
    if (-40.0..WIDTH + 40.0).contains(&position) {
        let mut actor = runner(0, position, direction, Depth::Foreground, frame.phase);
        actor.pose = Pose::Ride;
        actor.position.1 = 160.0;
        frame.actors.push(actor);
        if (0.0..WIDTH).contains(&position) {
            frame.coverage |= if direction == Direction::Left {
                motif::KART_LEFT
            } else {
                motif::KART_RIGHT
            };
        }
    }
}

pub fn valid_size((width, height): (f32, f32)) -> bool {
    width.is_finite()
        && height.is_finite()
        && width > 0.0
        && height > 0.0
        && width <= 8192.0
        && height <= 8192.0
}

fn rect(list: &mut Vec<Command>, size: (f32, f32), area: (f32, f32, f32, f32), color: [f32; 4]) {
    let (left, top, width, height) = area;
    let right = (left + width).clamp(0.0, WIDTH);
    let bottom = (top + height).clamp(0.0, HEIGHT);
    let left = left.clamp(0.0, WIDTH);
    let top = top.clamp(0.0, HEIGHT);
    if right <= left || bottom <= top {
        return;
    }
    let scale = (size.0 / WIDTH).min(size.1 / HEIGHT);
    let offset = (
        (size.0 - WIDTH * scale) / 2.0,
        (size.1 - HEIGHT * scale) / 2.0,
    );
    let (left, top, right, bottom) = (
        offset.0 + left * scale,
        offset.1 + top * scale,
        offset.0 + right * scale,
        offset.1 + bottom * scale,
    );
    list.push(Command {
        kind: Kind::Quad,
        color,
        points: [(left, top), (right, top), (right, bottom), (left, bottom)],
        size: 0.0,
        text: 0.0,
        layer: 0.0,
    });
}

fn kart_body(list: &mut Vec<Command>, size: (f32, f32), actor: &DecorativeActor, phase: f32) {
    let (position, baseline) = actor.position;
    let direction = if actor.direction == Direction::Right {
        1.0
    } else {
        -1.0
    };
    rect(
        list,
        size,
        (position - 34.0, baseline - 12.0, 68.0, 17.0),
        [0.13, 0.16, 0.16, 1.0],
    );
    rect(
        list,
        size,
        (position - 31.0, baseline - 9.0, 62.0, 11.0),
        [0.83, 0.23, 0.26, 1.0],
    );
    rect(
        list,
        size,
        (position - 24.0, baseline - 7.0, 48.0, 3.0),
        [1.0, 0.87, 0.59, 1.0],
    );
    rect(
        list,
        size,
        (
            position - direction * 29.0 - 3.0,
            baseline - 20.0,
            6.0,
            12.0,
        ),
        [0.94, 0.7, 0.23, 1.0],
    );
    rect(
        list,
        size,
        (
            position + direction * 18.0 - 3.0,
            baseline - 21.0,
            6.0,
            10.0,
        ),
        [0.17, 0.19, 0.18, 1.0],
    );
    for offset in [-22.0, 22.0] {
        let center = position + offset;
        rect(
            list,
            size,
            (center - 7.0, baseline, 14.0, 14.0),
            [0.1, 0.12, 0.12, 1.0],
        );
        rect(
            list,
            size,
            (center - 4.0, baseline + 3.0, 8.0, 8.0),
            [0.57, 0.63, 0.61, 1.0],
        );
        let angle = phase * 0.25 * direction;
        rect(
            list,
            size,
            (
                center + angle.cos() * 2.5 - 1.0,
                baseline + 6.0 + angle.sin() * 2.5,
                2.0,
                2.0,
            ),
            [0.96, 0.96, 0.9, 1.0],
        );
    }
}

pub fn commands(frame: &SceneFrame, size: (f32, f32)) -> Vec<Command> {
    if !valid_size(size) {
        return Vec::new();
    }
    let mut list = Vec::new();
    rect(
        &mut list,
        size,
        (0.0, 0.0, WIDTH, HEIGHT),
        [0.94, 0.96, 0.94, 1.0],
    );
    for (position, height) in [(56.0, 26.0), (144.0, 45.0), (376.0, 38.0), (524.0, 52.0)] {
        rect(
            &mut list,
            size,
            (position, 124.0 - height, 48.0, height),
            [0.81, 0.86, 0.83, 1.0],
        );
        rect(
            &mut list,
            size,
            (position + 6.0, 130.0 - height, 12.0, 12.0),
            [0.9, 0.93, 0.9, 1.0],
        );
    }
    rect(
        &mut list,
        size,
        (0.0, 124.0, WIDTH, 3.0),
        [0.66, 0.75, 0.68, 1.0],
    );
    rect(
        &mut list,
        size,
        (0.0, 174.0, WIDTH, 26.0),
        [0.24, 0.33, 0.28, 1.0],
    );
    rect(
        &mut list,
        size,
        (0.0, 174.0, WIDTH, 4.0),
        [0.63, 0.71, 0.38, 1.0],
    );
    for index in 0..21 {
        rect(
            &mut list,
            size,
            (index as f32 * 32.0, 181.0, 29.0, 10.0),
            [0.34, 0.43, 0.35, 1.0],
        );
        rect(
            &mut list,
            size,
            (index as f32 * 32.0 - 16.0, 194.0, 29.0, 6.0),
            [0.29, 0.39, 0.31, 1.0],
        );
    }
    for actor in &frame.actors {
        if actor.pose == Pose::Gone {
            continue;
        }
        let view = if actor.direction == Direction::Left {
            View::Left
        } else {
            View::Right
        };
        let pose = match actor.pose {
            Pose::StepA => Frame::StepA,
            Pose::StepB => Frame::StepB,
            Pose::Push => {
                if (frame.phase / 7.0) as u16 % 2 == 0 {
                    Frame::StepA
                } else {
                    Frame::StepB
                }
            }
            _ => Frame::Idle,
        };
        let (scale_x, scale_y) = actor.scale;
        let left = actor.position.0 - 6.0 * scale_x;
        let top = actor.position.1 - GRID_ROWS as f32 * scale_y;
        let ground = if actor.pose == Pose::Ride {
            174.0
        } else {
            actor.position.1
        };
        rect(
            &mut list,
            size,
            (left + 2.0, ground - 2.0, 10.0 * scale_x, 3.0),
            [0.1, 0.16, 0.12, 0.12 * actor.opacity],
        );
        let rows = if actor.pose == Pose::Ride {
            12
        } else {
            GRID_ROWS
        };
        for run in runs_view(&actor.look, view, pose, rows) {
            rect(
                &mut list,
                size,
                (
                    left + run.col as f32 * scale_x,
                    top + run.row as f32 * scale_y,
                    run.len as f32 * scale_x,
                    scale_y,
                ),
                [run.color[0], run.color[1], run.color[2], actor.opacity],
            );
        }
        if actor.pose == Pose::Push {
            rect(
                &mut list,
                size,
                (actor.position.0 + 5.0, actor.position.1 - 20.0, 20.0, 6.0),
                [0.96, 0.96, 0.94, actor.opacity],
            );
        }
        if actor.pose == Pose::Ride {
            kart_body(&mut list, size, actor, frame.phase);
        }
    }
    for bomb in &frame.bombs {
        if matches!(
            bomb.state,
            BombState::Blast | BombState::Cleared | BombState::Absent
        ) {
            continue;
        }
        let pixel = if bomb.role == BombRole::Incoming {
            4.0
        } else {
            2.5
        };
        for run in runs(&BOMB, BOMB.len(), |character| {
            bomb_colour(character, bomb.progress >= 0.6)
        }) {
            rect(
                &mut list,
                size,
                (
                    bomb.position.0 - 5.0 * pixel + run.col as f32 * pixel,
                    bomb.position.1 - 10.0 * pixel + run.row as f32 * pixel,
                    run.len as f32 * pixel,
                    pixel,
                ),
                [run.color[0], run.color[1], run.color[2], 1.0],
            );
        }
    }
    for effect in &frame.effects {
        let (position, extent, opacity) = (effect.position, effect.size, effect.opacity);
        if effect.kind != EffectKind::LocalBlast {
            for offset in [-1.0, 0.0, 1.0] {
                rect(
                    &mut list,
                    size,
                    (
                        position.0 + offset * extent - 5.0,
                        position.1 - 6.0 - offset.abs() * 7.0,
                        10.0,
                        8.0,
                    ),
                    [0.65, 0.68, 0.66, opacity],
                );
            }
            continue;
        }
        rect(
            &mut list,
            size,
            (position.0 - extent, position.1 - 10.0, extent * 2.0, 20.0),
            [0.93, 0.39, 0.15, opacity],
        );
        rect(
            &mut list,
            size,
            (position.0 - 10.0, position.1 - extent, 20.0, extent + 15.0),
            [1.0, 0.73, 0.17, opacity],
        );
        rect(
            &mut list,
            size,
            (position.0 - 8.0, position.1 - 8.0, 16.0, 16.0),
            [1.0, 0.94, 0.64, opacity],
        );
    }
    list
}
