//! Tunable game constants. Defaults follow specs/001-bomberman-multiplayer/data-model.md.

use crate::error::CoreError;

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
    pub powerup_chance: f32,
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
        if !(0.0..=1.0).contains(&self.block_density) || !(0.0..=1.0).contains(&self.powerup_chance)
        {
            return bad("block_density and powerup_chance must be in 0.0..=1.0");
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
