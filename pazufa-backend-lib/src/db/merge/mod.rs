pub mod candidates;
pub mod execute;

#[derive(Debug)]
pub enum MatchState<T> {
    Ambiguous(Vec<T>),
    ExactlyOne(T),
    NoMatch,
}
