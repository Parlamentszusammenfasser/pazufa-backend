#![cfg_attr(
    test,
    allow(clippy::expect_used, reason = "no allowline in every single test mod")
)]
mod conflicts;
mod matching;
mod regex;

pub use regex::Regex;

use snafu::Snafu;

#[derive(Debug, Snafu)]
pub enum TrackValidationError {
    IllegalExpression,
    RxParsingError,
    NotMatching,
}
