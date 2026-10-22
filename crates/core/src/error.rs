//! Typed errors for the rules crate.

use std::fmt;

/// Errors returned by the rules crate; the library never panics on bad input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreError {
    /// A slot number outside `0..4`.
    InvalidPlayerId(u8),
    /// A configuration value that breaks a rule.
    InvalidConfig(String),
    /// A match was created with an unusable set of slots.
    InvalidSlots(String),
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreError::InvalidPlayerId(i) => write!(f, "invalid player slot {i} (expected 0..4)"),
            CoreError::InvalidConfig(m) => write!(f, "invalid config: {m}"),
            CoreError::InvalidSlots(m) => write!(f, "invalid slots: {m}"),
        }
    }
}

impl std::error::Error for CoreError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_mentions_the_problem() {
        assert!(CoreError::InvalidPlayerId(9).to_string().contains('9'));
        assert!(CoreError::InvalidConfig("x".into())
            .to_string()
            .contains("x"));
    }
}
