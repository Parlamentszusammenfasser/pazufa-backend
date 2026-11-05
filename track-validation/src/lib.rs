#![cfg_attr(
    test,
    allow(clippy::expect_used, reason = "no allowline in every single test mod")
)]

use std::error::Error;
use std::fmt::Display;

mod conflicts;
mod nfa;
mod parser;

#[derive(Debug)]
pub enum TrackValidationError {
    UnknownCharacter(char, usize),
    TooLong,
    StarMisplaced,
    PlusMisplaced,
    QuestionMisplaced,
    BraceMismatch,
    BraceNotClosed,
}

impl Display for TrackValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BraceMismatch => write!(f, "Closing brace without previous open one"),
            Self::BraceNotClosed => write!(f, "Opening brace without closing one"),
            Self::UnknownCharacter(c, cidx) => {
                write!(f, "Unknown Character: {c} at index {cidx}")
            }
            Self::QuestionMisplaced => write!(f, "Question Mark without previous expression"),
            Self::PlusMisplaced => write!(f, "Plus without previous expression"),
            Self::StarMisplaced => write!(f, "Kleene Star without previous expression"),
            Self::TooLong => write!(f, "The regex is too long to pe parsed"),
        }
    }
}
impl Error for TrackValidationError {
    fn cause(&self) -> Option<&dyn Error> {
        None
    }
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
