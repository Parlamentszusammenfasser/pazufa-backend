#![allow(
    clippy::arithmetic_side_effects,
    reason = "there are too many for testing"
)]
use crate::parser::Parser;
use std::{collections::BTreeMap, fmt::Display};

// one byte per input: the sentinels below, or the ascii byte of an alphabetical character
// the derived ordering is: End < Start < Epsilon < Other < literals
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
struct InputChar(u8);

impl InputChar {
    const END: Self = Self(0);
    const START: Self = Self(1);
    const EPSILON: Self = Self(2);
    // stands in for any non-alphabetical input, no transition ever consumes it
    const INVALID: Self = Self(3);

    fn literal(c: char) -> Option<Self> {
        u8::try_from(c)
            .ok()
            .filter(u8::is_ascii_alphabetic)
            .map(Self)
    }
}

impl std::fmt::Debug for InputChar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::END => write!(f, "`End`"),
            Self::START => write!(f, "`Start`"),
            Self::EPSILON => write!(f, "`Epsilon`"),
            Self::INVALID => write!(f, "`Other`"),
            Self(byte) => write!(f, "'{}'", char::from(byte)),
        }
    }
}

#[derive(Debug, Default)]
struct State {
    transitions: BTreeMap<InputChar, Vec<usize>>,
}
#[derive(Debug)]
pub struct RegexNfa {
    states: Vec<State>,
}

impl Display for RegexNfa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut idx = 0;
        let output = self
            .states
            .iter()
            .fold("======NFA=====".to_owned(), |accu, st| {
                let out = format!(
                    "{accu}\n{}",
                    st.transitions
                        .iter()
                        .fold(format!("==State ({idx})=="), |accu, (k, v)| format!(
                            "{accu}\n{k:?} => {v:?}"
                        ))
                );
                idx += 1;
                out
            });
        write!(f, "{output}")
    }
}

impl RegexNfa {
    pub fn new(rx: &Parser) -> Self {
        let mut states = vec![State::default()];
        Self::thompson(rx, &mut states, 0);
        let mut nfa = Self { states };
        nfa.eliminate_epsilons();

        tracing::info!(
            "NFA Constructed with {} states, estimated size: {} bytes",
            nfa.states.len(),
            nfa.states.iter().fold(0usize, |accu, st| accu
                + std::mem::size_of_val(st)
                + st.transitions.len()
                    * (std::mem::size_of::<InputChar>() + std::mem::size_of::<usize>()))
        );
        tracing::debug!("{nfa}");
        nfa
    }
    fn push_state(states: &mut Vec<State>) -> usize {
        states.push(State::default());
        states.len() - 1
    }
    fn link(states: &mut [State], from: usize, input: InputChar, to: usize) {
        if let Some(thing) = states[from].transitions.get_mut(&input) {
            thing.push(to);
        } else {
            states[from].transitions.insert(input, vec![to]);
        }
    }
    // builds the automaton for `rx` starting at state `from`
    // returns: the index of the end state of the built fragment
    fn thompson(rx: &Parser, states: &mut Vec<State>, from: usize) -> usize {
        match rx {
            Parser::Empty => {
                let end = Self::push_state(states);
                Self::link(states, from, InputChar::EPSILON, end);
                end
            }
            Parser::Lit(c) => {
                let end = Self::push_state(states);
                // the parser only produces alphabetical literals,
                // anything else could never be matched anyway
                if let Some(input) = InputChar::literal(*c) {
                    Self::link(states, from, input, end);
                }
                end
            }
            Parser::Conc(vector) => vector.iter().fold(from, |current, sub_rx| {
                Self::thompson(sub_rx, states, current)
            }),
            Parser::Choice(rxa, rxb) => {
                let a_start = Self::push_state(states);
                let a_end = Self::thompson(rxa, states, a_start);
                let b_start = Self::push_state(states);
                let b_end = Self::thompson(rxb, states, b_start);
                let end = Self::push_state(states);

                Self::link(states, from, InputChar::EPSILON, a_start);
                Self::link(states, from, InputChar::EPSILON, b_start);
                Self::link(states, a_end, InputChar::EPSILON, end);
                Self::link(states, b_end, InputChar::EPSILON, end);
                end
            }
            Parser::Star(star) => {
                let star_start = Self::push_state(states);
                let star_end = Self::thompson(star, states, star_start);
                let end = Self::push_state(states);

                Self::link(states, from, InputChar::EPSILON, star_start);
                Self::link(states, from, InputChar::EPSILON, end);
                Self::link(states, star_end, InputChar::EPSILON, star_start);
                Self::link(states, star_end, InputChar::EPSILON, end);
                end
            }
        }
    }

    // all states reachable from `state` using only epsilon transitions, including `state`
    fn epsilon_closure(&self, state: usize) -> Vec<usize> {
        let mut seen = vec![false; self.states.len()];
        seen[state] = true;
        let mut closure = vec![];
        let mut stack = vec![state];
        while let Some(current) = stack.pop() {
            closure.push(current);
            if let Some(nexts) = self.states[current].transitions.get(&InputChar::EPSILON) {
                for &next in nexts {
                    if !seen[next] {
                        seen[next] = true;
                        stack.push(next);
                    }
                }
            }
        }
        closure
    }

    // eliminates all epsilon transitions, which turns the thompson automaton into the
    // glushkov automaton: only the start state and the states entered by a consuming
    // transition survive, and each of them takes over the consuming transitions of its
    // epsilon closure
    fn eliminate_epsilons(&mut self) {
        let len = self.states.len();
        let mut keep = vec![false; len];
        keep[0] = true;
        for state in &self.states {
            for (input, targets) in &state.transitions {
                if *input != InputChar::EPSILON {
                    for &target in targets {
                        keep[target] = true;
                    }
                }
            }
        }
        // surviving states keep their relative order, so the start state stays at 0
        let kept: Vec<usize> = (0..len).filter(|&state| keep[state]).collect();
        let mut new_index = vec![0; len];
        for (new, &old) in kept.iter().enumerate() {
            new_index[old] = new;
        }

        let reduced: Vec<State> = kept
            .iter()
            .map(|&old| {
                let mut state = State::default();
                for member in self.epsilon_closure(old) {
                    for (input, targets) in &self.states[member].transitions {
                        if *input == InputChar::EPSILON {
                            continue;
                        }
                        for &target in targets {
                            state
                                .transitions
                                .entry(*input)
                                .or_default()
                                .push(new_index[target]);
                        }
                    }
                }
                state
            })
            .collect();
        let reduced = Self::merge_dead_ends(reduced);
        tracing::info!(
            "Eliminated epsilon transitions, reduced NFA from {len} to {} states",
            reduced.len()
        );
        self.states = reduced;
    }

    // states without outgoing transitions cannot be told apart, since every state is
    // accepting, so all of them collapse into the first one. It has to survive, because
    // the transitions leading into it still consume input
    fn merge_dead_ends(states: Vec<State>) -> Vec<State> {
        let Some(sink) = states.iter().position(|state| state.transitions.is_empty()) else {
            return states;
        };
        // surviving states keep their relative order, so the start state stays at 0
        let mut removed = 0;
        let new_index: Vec<usize> = states
            .iter()
            .enumerate()
            .map(|(old, state)| {
                if old > sink && state.transitions.is_empty() {
                    removed += 1;
                    sink
                } else {
                    old - removed
                }
            })
            .collect();

        states
            .into_iter()
            .enumerate()
            .filter(|(old, state)| *old <= sink || !state.transitions.is_empty())
            .map(|(_, mut state)| {
                for targets in state.transitions.values_mut() {
                    // several targets may have collapsed into the same state
                    let mut merged = Vec::with_capacity(targets.len());
                    for &target in targets.iter() {
                        if !merged.contains(&new_index[target]) {
                            merged.push(new_index[target]);
                        }
                    }
                    *targets = merged;
                }
                state
            })
            .collect()
    }

    // returns: (matches, `consumed_input`)
    // where `consumed_input` excludes start and end
    pub fn matches(&self, input: &[char]) -> (bool, usize) {
        if input.is_empty() {
            return (true, 0);
        }
        let vec: Vec<InputChar> = std::iter::once(InputChar::START)
            .chain(
                input
                    .iter()
                    .map(|f| InputChar::literal(*f).unwrap_or(InputChar::INVALID)),
            )
            .chain(std::iter::once(InputChar::END))
            .collect();
        self.states[0].matches(&vec, &self.states)
    }
}

impl State {
    // assumes that input is never empty, but at the very least contains [Start, End]
    // returns: (matches, consumed_input)
    // where consumed_input excludes start and end
    fn matches(&self, input: &[InputChar], automaton: &Vec<State>) -> (bool, usize) {
        let current = &input[0];
        tracing::debug!("Matches called on state {self:?} with current: {current:?}");
        // we need no terminal check, since all states are accepting in prefix-matching mode.
        // If the input ended, we accept, and end is excluded
        if *current == InputChar::END {
            tracing::trace!("input char was end");
            (true, 0)
        // skip over start
        } else if *current == InputChar::START
            && let (mtch, consumed) = self.matches(&input[1..], automaton)
        {
            (mtch, consumed)
        // now we know the input is not empty, bc end has not been hit
        // thus, take the non-epsilon transitions and go for it; without epsilons there
        // may be several of them for the same input, so try them out one-by-one
        } else if let Some(states) = self.transitions.get(current) {
            let mut mx = 0;
            for next_state in states {
                let (mtch, consumed) = automaton[*next_state].matches(&input[1..], automaton);
                if mtch {
                    return (true, 1 + consumed);
                }
                mx = mx.max(consumed);
            }
            (false, 1 + mx)
        // if no consumptive transition is found, find the epsilon transitions
        // and try them out one-by-one
        } else if let Some(thing) = self.transitions.get(&InputChar::EPSILON) {
            tracing::trace!("trying all epsilon paths");
            let mut mx = 0;
            for next_state in thing {
                tracing::trace!("Epsilon");
                tracing::trace!("Next State: {next_state}");
                let (mtch, consumed) = automaton[*next_state].matches(input, automaton);
                if mtch {
                    return (true, consumed);
                }
                mx = mx.max(consumed);
            }
            tracing::debug!("exhausted epsilons; Leftover input: {input:?}");
            (false, mx)
        } else {
            tracing::debug!("no epsilons; Leftover input: {input:?}");
            (false, 0)
        }
    }
}

#[cfg(test)]
mod test {
    use super::{InputChar, RegexNfa, State};
    use crate::parser::Parser;

    const PATTERNS: [&str; 5] = [
        "(LA*)+LJ",
        "[RE]*S?I((VA*(Z|VJGK|VN|VA*(Z|VJGK|VN)))|Z)",
        "A*AB",
        "(AB|AC)D",
        "A(B|C)*D?",
    ];

    // returns: (thompson automaton, glushkov automaton)
    fn build(rx: &str) -> (RegexNfa, RegexNfa) {
        let chars: Vec<_> = rx.chars().collect();
        let parsed = Parser::new(&chars).expect("test case setup failed");
        let mut states = vec![State::default()];
        RegexNfa::thompson(&parsed, &mut states, 0);
        (RegexNfa { states }, RegexNfa::new(&parsed))
    }

    #[test]
    fn input_char_is_one_byte() {
        assert_eq!(std::mem::size_of::<InputChar>(), 1);
    }

    #[test]
    fn reduction_yields_glushkov_shape() {
        for rx in PATTERNS {
            let (thompson, glushkov) = build(rx);
            let literals = |nfa: &RegexNfa| -> usize {
                nfa.states
                    .iter()
                    .flat_map(|state| &state.transitions)
                    .filter(|(input, _)| **input != InputChar::EPSILON)
                    .map(|(_, targets)| targets.len())
                    .sum()
            };
            assert!(
                glushkov
                    .states
                    .iter()
                    .all(|state| !state.transitions.contains_key(&InputChar::EPSILON)),
                "epsilon left in {rx}"
            );
            // at most one state per literal occurrence, plus the start state
            assert!(glushkov.states.len() <= literals(&thompson) + 1, "{rx}");
            // all states without outgoing transitions are merged into one
            let dead_ends = glushkov
                .states
                .iter()
                .filter(|state| state.transitions.is_empty())
                .count();
            assert!(dead_ends <= 1, "{dead_ends} dead ends left in {rx}");
            assert!(
                glushkov
                    .states
                    .iter()
                    .flat_map(|state| state.transitions.values().flatten())
                    .all(|&target| target < glushkov.states.len()),
                "dangling transition in {rx}"
            );
        }
    }

    #[test]
    fn dead_ends_are_merged() {
        // start, the state after A (the parser factors the common prefix), and one shared
        // end for B, C and D
        assert_eq!(build("AB|AC|D").1.states.len(), 3);
        // Z, K and N all end the pattern
        let (_, glushkov) = build("I(Z|VJGK|VN)");
        assert_eq!(glushkov.states.len(), 6);
    }

    #[test]
    fn reduction_preserves_matching() {
        let inputs = [
            "", "A", "B", "L", "R", "AB", "AAB", "AAAB", "ABD", "ACD", "AD", "ABCBD", "ABX", "LLJ",
            "LALAALJ", "LJX", "RESI", "REIVAAZ", "IVVAZ", "IVJGK", "IZ", "SIVN", "RX", "IVAX",
        ];
        for rx in PATTERNS {
            let (thompson, glushkov) = build(rx);
            for input in inputs {
                let chars: Vec<_> = input.chars().collect();
                assert_eq!(
                    thompson.matches(&chars),
                    glushkov.matches(&chars),
                    "{rx} on {input:?}"
                );
            }
        }
    }
}
