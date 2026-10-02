#![cfg_attr(
    test,
    allow(clippy::expect_used, reason = "no allowline in every single test mod")
)]
mod conflicts;
mod matching;
mod nfa;
mod parser;

use snafu::Snafu;

#[derive(Debug, Snafu)]
pub enum TrackValidationError {
    IllegalExpression,
    RxParsingError,
    NotMatching,
}

pub struct Regex {
    pub(crate) nfa: nfa::RegexNfa,
}
impl Regex {
    /// Creates a new regex from a str
    /// # Errors
    /// Can fail if the parsing of the chars as regex fails
    pub fn new(s: &str) -> Result<Self, TrackValidationError> {
        let x: Vec<_> = s.chars().collect();
        let rx = parser::Parser::new(&x)?;
        Ok(Self {
            nfa: nfa::RegexNfa::new(&rx),
        })
    }
    #[must_use]
    pub fn matches(&self, s: &str) -> (bool, usize) {
        let input: Vec<_> = s.chars().collect();
        self.nfa.matches(&input)
    }
}
