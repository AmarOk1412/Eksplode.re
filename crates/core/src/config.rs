//! Tunable game constants. Defaults follow specs/001-bomberman-multiplayer/data-model.md.

use crate::error::CoreError;
use crate::room::Mode;

/// All tunable rule values of a match.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Config {
    pub width: u8,
    pub height: u8,
    pub tick_hz: u32,
    pub fuse_ticks: u32,
    pub flame_ticks: u32,
    pub round_limit_ticks: u32,
    pub base_move_cooldown: u32,
    pub min_move_cooldown: u32,
    pub start_bombs: u8,
    pub start_range: u8,
    pub max_bombs: u8,
    pub max_range: u8,
    pub block_density: f32,
    /// Share of destroyed blocks that drop a power (the default drop table adds up to this).
    pub powerup_chance: f32,
    /// How long a malus lasts, in ticks.
    #[cfg_attr(feature = "serde", serde(default = "default_malus_ticks"))]
    pub malus_ticks: u32,
    /// Ticks a punched bomb needs for one hop.
    #[cfg_attr(feature = "serde", serde(default = "default_punch_hop_ticks"))]
    pub punch_hop_ticks: u32,
    /// Ticks a kicked bomb needs for one square.
    #[cfg_attr(feature = "serde", serde(default = "default_slide_ticks"))]
    pub slide_ticks: u32,
    /// How the round is played.
    #[cfg_attr(feature = "serde", serde(default))]
    pub mode: Mode,
    /// Whether explosions hurt teammates (Team mode).
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub friendly_fire: bool,
    /// Length of the end pattern before the time limit; 0 turns it off. A pattern that is not
    /// shorter than the round limit never runs, and the limit ends the round as a draw.
    #[cfg_attr(feature = "serde", serde(default = "default_closing_ticks"))]
    pub closing_ticks: u32,
    /// Ticks a falling block is announced before it lands.
    #[cfg_attr(feature = "serde", serde(default = "default_closing_lead_ticks"))]
    pub closing_lead_ticks: u32,
    /// Bombs the chaser starts with.
    #[cfg_attr(feature = "serde", serde(default = "default_chaser_bombs"))]
    pub chaser_bombs: u8,
    /// Hearts the chaser starts with.
    #[cfg_attr(feature = "serde", serde(default = "default_chaser_hearts"))]
    pub chaser_hearts: u8,
    /// Ticks the chaser is protected after losing a heart.
    #[cfg_attr(feature = "serde", serde(default = "default_shield_ticks"))]
    pub shield_ticks: u32,
    /// The Chaser arena keeps the pillars where `(x / 2 + y / 2) % chaser_pillar_mod == 0`.
    #[cfg_attr(feature = "serde", serde(default = "default_chaser_pillar_mod"))]
    pub chaser_pillar_mod: u8,
    /// Share of free tiles that hold a block in the Chaser arena.
    #[cfg_attr(feature = "serde", serde(default = "default_chaser_block_density"))]
    pub chaser_block_density: f32,
    /// Ticks between two bonuses in Chaser mode.
    #[cfg_attr(feature = "serde", serde(default = "default_bonus_every_ticks"))]
    pub bonus_every_ticks: u32,
    /// Most bonuses lying on the board at once in Chaser mode.
    #[cfg_attr(feature = "serde", serde(default = "default_bonus_cap"))]
    pub bonus_cap: u8,
}

#[cfg(feature = "serde")]
fn default_true() -> bool {
    true
}

#[cfg(feature = "serde")]
fn default_closing_ticks() -> u32 {
    1200
}

#[cfg(feature = "serde")]
fn default_closing_lead_ticks() -> u32 {
    20
}

#[cfg(feature = "serde")]
fn default_chaser_bombs() -> u8 {
    5
}

#[cfg(feature = "serde")]
fn default_chaser_hearts() -> u8 {
    3
}

#[cfg(feature = "serde")]
fn default_shield_ticks() -> u32 {
    40
}

#[cfg(feature = "serde")]
fn default_chaser_pillar_mod() -> u8 {
    3
}

#[cfg(feature = "serde")]
fn default_chaser_block_density() -> f32 {
    0.12
}

#[cfg(feature = "serde")]
fn default_bonus_every_ticks() -> u32 {
    100
}

#[cfg(feature = "serde")]
fn default_bonus_cap() -> u8 {
    6
}

#[cfg(feature = "serde")]
fn default_malus_ticks() -> u32 {
    200
}

#[cfg(feature = "serde")]
fn default_punch_hop_ticks() -> u32 {
    2
}

#[cfg(feature = "serde")]
fn default_slide_ticks() -> u32 {
    3
}

impl Default for Config {
    fn default() -> Self {
        Self {
            width: 15,
            height: 13,
            tick_hz: 20,
            fuse_ticks: 60,
            flame_ticks: 10,
            round_limit_ticks: 3600,
            base_move_cooldown: 4,
            min_move_cooldown: 2,
            start_bombs: 1,
            start_range: 2,
            max_bombs: 6,
            max_range: 8,
            block_density: 0.7,
            powerup_chance: 0.3,
            malus_ticks: 200,
            punch_hop_ticks: 2,
            slide_ticks: 3,
            mode: Mode::Normal,
            friendly_fire: true,
            closing_ticks: 1200,
            closing_lead_ticks: 20,
            chaser_bombs: 5,
            chaser_hearts: 3,
            shield_ticks: 40,
            chaser_pillar_mod: 3,
            chaser_block_density: 0.12,
            bonus_every_ticks: 100,
            bonus_cap: 6,
        }
    }
}

impl Config {
    /// Checks the invariants the rules rely on.
    pub fn validate(&self) -> Result<(), CoreError> {
        let bad = |m: &str| Err(CoreError::InvalidConfig(m.to_string()));
        if self.width < 7 || self.height < 7 {
            return bad("arena must be at least 7x7");
        }
        if self.width % 2 == 0 || self.height % 2 == 0 {
            return bad("arena width and height must be odd");
        }
        if self.tick_hz == 0 {
            return bad("tick_hz must be positive");
        }
        if self.fuse_ticks == 0 || self.flame_ticks == 0 {
            return bad("fuse_ticks and flame_ticks must be positive");
        }
        if self.min_move_cooldown == 0 || self.min_move_cooldown > self.base_move_cooldown {
            return bad("min_move_cooldown must be in 1..=base_move_cooldown");
        }
        if self.start_bombs == 0 || self.start_bombs > self.max_bombs {
            return bad("start_bombs must be in 1..=max_bombs");
        }
        if self.start_range == 0 || self.start_range > self.max_range {
            return bad("start_range must be in 1..=max_range");
        }
        if self.malus_ticks == 0 || self.punch_hop_ticks == 0 || self.slide_ticks == 0 {
            return bad("malus_ticks, punch_hop_ticks and slide_ticks must be positive");
        }
        if !(0.0..=1.0).contains(&self.block_density) || !(0.0..=1.0).contains(&self.powerup_chance)
        {
            return bad("block_density and powerup_chance must be in 0.0..=1.0");
        }
        if self.closing_lead_ticks == 0 {
            return bad("closing_lead_ticks must be positive");
        }
        if self.chaser_hearts == 0 || self.chaser_bombs == 0 {
            return bad("chaser_hearts and chaser_bombs must be positive");
        }
        if self.chaser_pillar_mod == 0 || self.bonus_every_ticks == 0 {
            return bad("chaser_pillar_mod and bonus_every_ticks must be positive");
        }
        if !(0.0..=1.0).contains(&self.chaser_block_density) {
            return bad("chaser_block_density must be in 0.0..=1.0");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_valid() {
        assert_eq!(Config::default().validate(), Ok(()));
    }

    #[test]
    fn rejects_even_or_small_arenas() {
        let even = Config {
            width: 14,
            ..Config::default()
        };
        assert!(even.validate().is_err());
        let small = Config {
            height: 5,
            ..Config::default()
        };
        assert!(small.validate().is_err());
    }

    #[test]
    fn rejects_inverted_caps() {
        let c = Config {
            start_bombs: 7,
            ..Config::default()
        };
        assert!(c.validate().is_err());
        let c = Config {
            start_range: 9,
            ..Config::default()
        };
        assert!(c.validate().is_err());
        let c = Config {
            min_move_cooldown: 5,
            ..Config::default()
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn rejects_bad_probabilities() {
        let c = Config {
            block_density: 1.5,
            ..Config::default()
        };
        assert!(c.validate().is_err());
    }
}
