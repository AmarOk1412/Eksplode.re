//! The chance that a destroyed block drops each kind of power.

use crate::error::CoreError;
use crate::rng::Rng;
use crate::state::PowerUpKind;

/// Shares of the overall drop rate, in [`PowerUpKind::ALL`] order: extra bomb, flame, roller,
/// punch, slow, hyper speed, small bomb. They add up to 1.0.
pub const DEFAULT_SHARES: [f32; 7] = [0.20, 0.24, 0.24, 0.03, 0.10, 0.08, 0.11];

/// One chance per power kind, in [`PowerUpKind::ALL`] order.
///
/// Every chance is finite and within `0.0..=1.0`. When they add up to more than 1.0 they are
/// scaled down proportionally when rolling, so a block never drops more than one item.
#[derive(Clone, Debug, PartialEq)]
pub struct DropTable {
    pub chances: [f32; 7],
}

impl DropTable {
    /// The default table for an overall drop rate: each kind gets its share of `overall`.
    pub fn defaults(overall: f32) -> DropTable {
        let overall = if overall.is_finite() {
            overall.clamp(0.0, 1.0)
        } else {
            0.0
        };
        DropTable {
            chances: DEFAULT_SHARES.map(|share| share * overall),
        }
    }

    /// A table with the given chances; each must be finite and within `0.0..=1.0`.
    pub fn new(chances: [f32; 7]) -> Result<DropTable, CoreError> {
        let table = DropTable { chances };
        table.validate()?;
        Ok(table)
    }

    /// Checks that every chance is finite and within `0.0..=1.0`.
    pub fn validate(&self) -> Result<(), CoreError> {
        if self
            .chances
            .iter()
            .all(|c| c.is_finite() && (0.0..=1.0).contains(c))
        {
            Ok(())
        } else {
            Err(CoreError::InvalidConfig(
                "drop chances must be finite and in 0.0..=1.0".into(),
            ))
        }
    }

    /// The chance of `kind`, before any scaling.
    pub fn chance(&self, kind: PowerUpKind) -> f32 {
        PowerUpKind::ALL
            .iter()
            .position(|k| *k == kind)
            .map_or(0.0, |i| self.chances[i])
    }

    /// The sum of all chances.
    pub fn total(&self) -> f32 {
        self.chances.iter().sum()
    }

    /// Decides what a destroyed block drops: one draw from `rng`, at most one item.
    pub fn roll(&self, rng: &mut Rng) -> Option<PowerUpKind> {
        let total = self.total();
        let scale = if total > 1.0 { 1.0 / total } else { 1.0 };
        let x = rng.next_unit();
        let mut acc = 0.0;
        let mut last = None;
        for (kind, chance) in PowerUpKind::ALL.iter().zip(self.chances) {
            if chance > 0.0 {
                last = Some(*kind);
            }
            acc += chance * scale;
            if x < acc {
                return Some(*kind);
            }
        }
        // rounding can leave a sliver below 1.0 when the table adds up to (about) one
        if total * scale >= 0.999_999 {
            return last;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shares_add_up_to_one() {
        assert!((DEFAULT_SHARES.iter().sum::<f32>() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_non_finite_overall_rate_gives_an_empty_table() {
        assert_eq!(DropTable::defaults(f32::NAN).total(), 0.0);
    }
}
