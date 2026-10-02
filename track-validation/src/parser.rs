use crate::TrackValidationError;
use std::cmp::Ordering;
use std::collections::VecDeque;
use std::mem;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parser {
    Star(Box<Parser>),
    Choice(Box<Parser>, Box<Parser>),
    Conc(Vec<Parser>),
    Lit(char),
    Empty,
}

impl PartialOrd for Parser {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(Ord::cmp(&self, &other))
    }
}

impl Ord for Parser {
    fn cmp(&self, other: &Self) -> Ordering {
        let rank = |re: &Parser| match re {
            Parser::Empty => 0,
            Parser::Lit(_) => 1,
            Parser::Star(_) => 2,
            Parser::Conc(_) => 3,
            Parser::Choice(_, _) => 4,
        };

        match (self, other) {
            (Parser::Lit(x), Parser::Lit(y)) => x.cmp(y),
            (Parser::Star(x), Parser::Star(y)) => x.cmp(y),
            (Parser::Conc(xs), Parser::Conc(ys)) => xs.len().cmp(&ys.len()).then_with(|| {
                xs.iter()
                    .zip(ys.iter())
                    .map(|(x, y)| x.cmp(y))
                    .find(|o| *o != Ordering::Equal)
                    .unwrap_or(Ordering::Equal)
            }),
            // Choices should be flattened before we get here,
            // but fall back to structural comparison
            (Parser::Choice(a1, a2), Parser::Choice(b1, b2)) => a1.cmp(b1).then_with(|| a2.cmp(b2)),
            _ => rank(self).cmp(&rank(other)),
        }
    }
}

impl Parser {
    pub fn new(rx: &[char]) -> Result<Self, TrackValidationError> {
        if rx.len() == usize::MAX {
            Err(TrackValidationError::RxParsingError)
        } else {
            Self::parse_regex(rx)
        }
    }

    // takes `[ABCDEF]` returns Parser("A|B|C|D|E|F")
    #[allow(
        clippy::arithmetic_side_effects,
        reason = "only overflows for re.len() == usize::MAX, which is asserted out before"
    )]
    fn parse_choicebox(re: &[char]) -> (Parser, usize) {
        let mut last_regex = Parser::Empty;
        let mut ci = 0;

        while ci < re.len() {
            let c = re[ci];
            match (c, &mut last_regex) {
                ('[', _) => {}
                (']', _) => break,
                ('A'..='Z', Parser::Empty) => last_regex = Parser::Lit(c),
                ('A'..='Z', Parser::Lit(_)) => {
                    last_regex = Parser::Choice(Parser::Lit(c).into(), last_regex.into());
                }
                ('A'..='Z', Parser::Choice(_, _)) => {
                    last_regex = Parser::Choice(last_regex.into(), Parser::Lit(c).into());
                }
                _ => panic!(),
            }
            ci += 1;
        }
        (last_regex, ci + 1)
    }

    /// LIMITATION: parses a*aa into a*aa, and does no reordering.
    /// This assumes that stars are always at the end of expressions of the same kind:
    /// a*aa <=> aaa*
    #[allow(
        clippy::arithmetic_side_effects,
        reason = "only overflows for re.len() == usize::MAX, which is asserted out before"
    )]
    fn parse_regex(re: &[char]) -> Result<Parser, TrackValidationError> {
        if re.is_empty() {
            return Ok(Parser::Empty);
        }

        let mut rx = Parser::Conc(vec![]);
        let mut ci = 0;

        while ci < re.len() {
            let c = re[ci];
            match (c, &mut rx) {
                ('A'..='Z', Parser::Conc(child)) => child.push(Parser::Lit(c)),
                ('*', Parser::Conc(child)) => {
                    if let Some(last_child) = child.pop() {
                        child.push(Parser::Star(last_child.into()));
                    } else {
                        return Err(TrackValidationError::RxParsingError);
                    }
                }
                ('|', Parser::Conc(_)) => {
                    let rx_slimmed = match rx {
                        Parser::Conc(v) => Parser::from_parts(v),
                        _ => rx.clone(),
                    };
                    rx =
                        Parser::Choice(rx_slimmed.into(), Self::parse_regex(&re[ci + 1..])?.into());
                    ci = re.len();
                    continue;
                }
                ('(', Parser::Conc(child)) => {
                    let subre = Self::find_matching_brace(&re[ci..])?;
                    tracing::trace!("matching brace: {:?}", subre);
                    ci += subre.len() + 1;
                    child.push(Self::parse_regex(subre)?);
                }
                ('[', Parser::Conc(child)) => {
                    let (cbrx, cblen) = Self::parse_choicebox(&re[ci..]);
                    ci += cblen - 1;
                    child.push(cbrx);
                }
                ('?', Parser::Conc(child)) => {
                    if let Some(last_child) = child.pop() {
                        child.push(Parser::Choice(last_child.into(), Parser::Empty.into()));
                    } else {
                        return Err(TrackValidationError::RxParsingError);
                    }
                }
                ('+', Parser::Conc(child)) => {
                    if let Some(last_child) = child.last() {
                        child.push(Parser::Star(Box::new(last_child.clone())));
                    } else {
                        return Err(TrackValidationError::RxParsingError);
                    }
                }
                _ => {
                    return Err(TrackValidationError::RxParsingError);
                }
            }
            ci += 1;
        }

        let mut rx = match rx {
            Parser::Conc(v) => Parser::from_parts(v),
            _ => rx,
        };

        loop {
            let new = rx.clone().simplify();

            if rx == new {
                break Ok(rx);
            }
            rx = new;
        }
    }

    /// takes `(abc(de)fg(h))` and returns `abc(de)fg(h)`
    #[allow(
        clippy::arithmetic_side_effects,
        reason = "brace_ident cannot exceed re.len()"
    )]
    fn find_matching_brace(re: &[char]) -> Result<&[char], TrackValidationError> {
        let mut brace_indent = 0;
        for ci in 0..re.len() {
            match re[ci] {
                '(' => brace_indent += 1,
                ')' => brace_indent -= 1,
                _ => {}
            }
            if brace_indent == 0 {
                return Ok(&re[1..ci]);
            }
        }
        Ok(&re[1..re
            .len()
            .checked_sub(1)
            .ok_or(TrackValidationError::RxParsingError)?])
    }
    fn simplify(self) -> Self {
        match self {
            Self::Star(inner) => Self::Star(inner.simplify().into()),
            Self::Choice(left, right) => {
                if *left == Self::Empty && *right == Self::Empty {
                    Self::Empty
                } else {
                    Self::Choice(left.simplify().into(), right.simplify().into())
                        .canonicalize_choice()
                        .factor_prefix()
                }
            }
            Self::Conc(inner) => Self::Conc(inner.into_iter().map(Self::simplify).collect()),
            other => other,
        }
        .reduce()
    }
    fn reduce(self) -> Self {
        match self {
            Self::Star(inner) if *inner == Parser::Empty => Parser::Empty,
            Self::Star(inner) if matches!(*inner, Parser::Star(_)) => *inner,
            Self::Choice(left, right) if left == right => *left,
            Self::Choice(left, right)
                if *left == Parser::Empty && matches!(*right, Parser::Star(_)) =>
            {
                *right
            }
            Self::Choice(left, right)
                if *right == Parser::Empty && matches!(*left, Parser::Star(_)) =>
            {
                *left
            }
            Self::Choice(left, right)
                if matches!(*right, Parser::Star(_)) && {
                    if let Parser::Star(inner) = &*right {
                        **inner == *left
                    } else {
                        false
                    }
                } =>
            {
                *right
            }
            Self::Choice(left, right)
                if matches!(*left, Self::Star(_)) && {
                    if let Self::Star(inner) = &*left {
                        **inner == *right
                    } else {
                        false
                    }
                } =>
            {
                *left
            }

            Parser::Conc(inner) => Self::reduce_conc(inner),
            other => other,
        }
    }

    fn from_parts(mut parts: Vec<Parser>) -> Parser {
        if parts.len() == 1 {
            let Some(head) = parts.pop() else {
                unreachable!()
            };
            head
        } else if parts.is_empty() {
            Self::Empty
        } else {
            Self::Conc(parts)
        }
    }
    fn canonicalize_choice(self) -> Parser {
        fn flatten_choice(re: Parser) -> Vec<Parser> {
            match re {
                Parser::Choice(left, right) => {
                    let mut alts = flatten_choice(*left);
                    alts.extend(flatten_choice(*right));
                    alts
                }
                other => vec![other],
            }
        }

        let mut flattened = flatten_choice(self);
        flattened.sort();
        flattened.dedup();

        flattened
            .into_iter()
            .rev()
            .reduce(|acc, item| Self::Choice(item.into(), acc.into()))
            .unwrap_or(Parser::Empty)
    }

    fn factor_prefix(self) -> Parser {
        if let Self::Choice(left, right) = self {
            let left: &mut dyn Iterator<Item = Parser> = if let Self::Conc(left) = *left {
                &mut left.into_iter()
            } else {
                &mut std::iter::once(*left)
            };
            let right: &mut dyn Iterator<Item = Parser> = if let Self::Conc(right) = *right {
                &mut right.into_iter()
            } else {
                &mut std::iter::once(*right)
            };

            let mut prefix = vec![];
            let mut new_left = vec![];
            let mut new_right = vec![];
            #[allow(
                clippy::while_let_on_iterator,
                reason = "Alternative writing is not supported for dyn objects."
            )]
            while let Some(left) = left.next() {
                if let Some(right) = right.next() {
                    if left == right && left != Parser::Empty {
                        prefix.push(left);
                    } else {
                        new_left.push(left);
                        new_right.push(right);
                        break;
                    }
                } else if left == Parser::Empty {
                    // do nothing
                } else {
                    new_left.push(left);
                }
            }
            new_left.extend(left);
            new_right.extend(right);

            let new_choice = Self::Choice(
                Self::from_parts(new_left).into(),
                Self::from_parts(new_right).into(),
            );

            if prefix.is_empty() {
                new_choice
            } else {
                prefix.push(new_choice);
                Self::from_parts(prefix)
            }
        } else {
            self
        }
    }
    fn reduce_conc(parts: Vec<Parser>) -> Parser {
        let mut flat = VecDeque::new();
        for part in parts {
            if let Self::Conc(inner) = part {
                flat.extend(inner);
            } else {
                flat.push_back(part);
            }
        }

        flat.retain(|r| *r != Self::Empty);

        let mut absorbed = Vec::with_capacity(flat.len());
        while let Some(head) = flat.pop_front() {
            let head = if let Some(next) = flat.get_mut(0) {
                match (head, next) {
                    (mut current, next)
                        if matches!(current, Self::Star(_)) && {
                            if let Self::Star(inner) = &current {
                                **inner == *next
                            } else {
                                false
                            }
                        } =>
                    {
                        mem::swap(&mut current, next);
                        current
                    }
                    (Self::Star(head), Self::Star(next)) if *head == **next => {
                        continue;
                    }
                    (head, _) => head,
                }
            } else {
                head
            };
            absorbed.push(head);
        }

        Self::from_parts(absorbed)
    }
}

#[cfg(test)]
mod regression_test {
    use super::*;

    #[test]
    fn issue89() {
        let chr: char = '\u{1075cc}';

        let parts = std::iter::repeat_n(Parser::Star(Parser::Lit(chr).into()), 1)
            .chain(std::iter::repeat_n(Parser::Empty, 1))
            .collect::<Vec<_>>();

        let reduced = Parser::reduce_conc(parts);

        let expected = Parser::Star(Parser::Lit(chr).into());

        assert_eq!(reduced, expected);
    }

    #[test]
    fn issue44_empty_regex() {
        let mut parts = vec![];

        let re = Parser::from_parts(parts.clone());

        parts.push(Parser::Choice(Parser::Empty.into(), Parser::Empty.into()));
        let expected = Parser::from_parts(parts);

        assert_eq!(
            Parser::Choice(re.clone().into(), re.clone().into(),).factor_prefix(),
            expected
        );
    }
    #[test]
    fn issue45_empty_suffix() {
        let pool = vec![Parser::Lit('\u{623cd}')];
        let prefix = pool.into_iter().take(1).collect::<Vec<_>>();
        // This is the interesting part: suffix empty means the reduction is one level up
        let suffix = [];
        let longer = prefix
            .iter()
            .cloned()
            .chain(suffix.iter().cloned())
            .collect::<Vec<_>>();

        let left_long = Parser::Choice(
            Parser::from_parts(longer.clone()).into(),
            Parser::from_parts(prefix.clone()).into(),
        );
        let right_long = Parser::Choice(
            Parser::from_parts(prefix.clone()).into(),
            Parser::from_parts(longer).into(),
        );

        let expected = Parser::from_parts(prefix);

        assert_eq!(
            left_long.factor_prefix().simplify(),
            expected,
            "Failed to correctly factor prefix if right is prefix of left. Other way not tested."
        );
        assert_eq!(
            right_long.factor_prefix().simplify(),
            expected,
            "Failed to correctly factor prefix if left is prefix of right. Other way succeeded."
        );
    }

    #[test]
    fn issue45_empty_prefix() {
        let pool = "abop".chars().map(Parser::Lit);
        let mut prefix = pool.clone().take(0).collect::<Vec<_>>();
        let suffix = pool.rev().take(3).collect::<Vec<_>>();
        let longer = prefix
            .iter()
            .cloned()
            .chain(suffix.iter().cloned())
            .collect::<Vec<_>>();

        let left_long = Parser::Choice(
            Parser::from_parts(longer.clone()).into(),
            Parser::from_parts(prefix.clone()).into(),
        );

        // building `expected`
        if !suffix.is_empty() {
            prefix.push(Parser::Choice(
                Parser::Empty.into(),
                Parser::from_parts(suffix).into(),
            ));
        }
        let expected = Parser::from_parts(prefix);

        assert_eq!(
            left_long.factor_prefix().simplify(),
            expected,
            "Failed to correctly factor prefix if right is prefix of left. Other way not tested."
        );
    }
}

#[cfg(test)]
#[allow(
    clippy::arithmetic_side_effects,
    reason = "is either checked in test or can panic"
)]
mod test {
    use super::*;
    use rand::{RngExt, rng, seq::SliceRandom};

    #[test]
    fn plain_chars() {
        let input = "ABC";
        let input: Vec<char> = input.chars().collect();
        let output = Parser::Conc(vec![Parser::Lit('A'), Parser::Lit('B'), Parser::Lit('C')]);
        assert_eq!(output, Parser::parse_regex(&input).expect("const input"));
    }

    #[test]
    fn star_on_char() {
        let input = "BA*";
        let input: Vec<char> = input.chars().collect();
        let output = Parser::Conc(vec![
            Parser::Lit('B'),
            Parser::Star(Box::new(Parser::Lit('A'))),
        ]);
        assert_eq!(output, Parser::parse_regex(&input).expect("const input"));
    }

    #[test]
    fn choice_concat() {
        let input = "BB|AA";
        let input: Vec<char> = input.chars().collect();
        let output = Parser::Choice(
            Parser::Conc(vec![Parser::Lit('A'), Parser::Lit('A')]).into(),
            Parser::Conc(vec![Parser::Lit('B'), Parser::Lit('B')]).into(),
        );
        assert_eq!(output, Parser::parse_regex(&input).expect("const input"));
    }

    #[test]
    fn choice() {
        let input = "B|A";
        let input: Vec<char> = input.chars().collect();
        let output = Parser::Choice(Box::new(Parser::Lit('A')), Box::new(Parser::Lit('B')));
        assert_eq!(output, Parser::parse_regex(&input).expect("const input"));
    }

    #[test]
    fn choice_braces() {
        let input = "B|(AB)";
        let input: Vec<char> = input.chars().collect();
        let output = Parser::Choice(
            Box::new(Parser::Lit('B')),
            Box::new(Parser::Conc(vec![Parser::Lit('A'), Parser::Lit('B')])),
        );
        assert_eq!(output, Parser::parse_regex(&input).expect("const input"));
    }

    #[test]
    fn brace_then_star() {
        let input = "(AB)*";
        let input: Vec<char> = input.chars().collect();
        let output = Parser::Star(Parser::Conc(vec![Parser::Lit('A'), Parser::Lit('B')]).into());
        assert_eq!(output, Parser::parse_regex(&input).expect("const input"));
    }

    #[test]
    fn choicebox_parsing() {
        let input = "[ABC]";
        let input: Vec<char> = input.chars().collect();
        let output = Parser::Choice(
            Parser::Choice(Parser::Lit('B').into(), Parser::Lit('A').into()).into(),
            Parser::Lit('C').into(),
        );

        assert_eq!((output, 5), Parser::parse_choicebox(&input));
    }
    #[test]
    fn choicebox_after_star() {
        let input = "A*[BC]";
        let input: Vec<char> = input.chars().collect();
        let input = Parser::parse_regex(&input).expect("Static value unwrap");

        let output = Parser::Conc(vec![
            Parser::Star(Parser::Lit('A').into()),
            Parser::Choice(Parser::Lit('B').into(), Parser::Lit('C').into()),
        ]);

        assert_eq!(output, input);
    }
    #[test]
    fn choiceboxstar() {
        let input = "[ABC]*";
        let input: Vec<char> = input.chars().collect();
        let output = Parser::Star(
            Parser::Choice(
                Parser::Lit('A').into(),
                Parser::Choice(Parser::Lit('B').into(), Parser::Lit('C').into()).into(),
            )
            .into(),
        );
        assert_eq!(output, Parser::parse_regex(&input).expect("const input"));
    }

    #[test]
    fn questionmark() {
        let input = "A?";
        let input: Vec<char> = input.chars().collect();
        let output = Parser::Choice(Parser::Empty.into(), Parser::Lit('A').into());
        assert_eq!(output, Parser::parse_regex(&input).expect("const input"));
    }
    #[test]
    fn plus() {
        let input = "A+";
        let input: Vec<char> = input.chars().collect();
        let output = Parser::Conc(vec![
            Parser::Lit('A'),
            Parser::Star(Parser::Lit('A').into()),
        ]);
        assert_eq!(output, Parser::parse_regex(&input).expect("const input"));
    }

    #[test]
    fn test_canonicalizes_choicebox() {
        let mut r = rng();
        let mut options = ['A', 'B', 'C', 'D', 'E', 'F'];
        options.shuffle(&mut r);

        let randomized_choice = match options
            .iter()
            .fold(Parser::Empty, |acc, e| {
                if r.random_bool(0.5) {
                    Parser::Choice(acc.into(), Parser::Lit(*e).into())
                } else {
                    Parser::Choice(Parser::Lit(*e).into(), acc.into())
                }
            })
            .canonicalize_choice()
        {
            Parser::Choice(left, right) if *left == Parser::Empty => *right,
            e => panic!(
                "Canonization did not return Choice(Empty, canonized) as it was expected. Actual result: {e:?}"
            ),
        };

        let parsed_choice =
            Parser::new(&"[ABCDEF]".chars().collect::<Vec<_>>()).expect("const input");

        let expected = Parser::Choice(
            Parser::Lit('A').into(),
            Parser::Choice(
                Parser::Lit('B').into(),
                Parser::Choice(
                    Parser::Lit('C').into(),
                    Parser::Choice(
                        Parser::Lit('D').into(),
                        Parser::Choice(Parser::Lit('E').into(), Parser::Lit('F').into()).into(),
                    )
                    .into(),
                )
                .into(),
            )
            .into(),
        );

        assert_eq!(
            randomized_choice, expected,
            "Randomized Choice did not canonenize correctly."
        );
        assert_eq!(
            parsed_choice, expected,
            "Parsed Choice did not canonenize correctly."
        );
        assert_eq!(
            randomized_choice, parsed_choice,
            "Randomized and Parsed Choice did not converge on identical canonenization."
        );
    }

    #[test]
    fn canonicalizes_arbitrary_choices() {
        let mut r = rng();
        let len: usize = r.random_range(32..64);
        let mut options = (0..len).map(|_| r.random::<char>()).collect::<Vec<_>>();

        options.shuffle(&mut r);

        let first_shuffle = match options
            .iter()
            .fold(Parser::Empty, |acc, e| {
                if r.random_bool(0.5) {
                    Parser::Choice(acc.into(), Parser::Lit(*e).into())
                } else {
                    Parser::Choice(Parser::Lit(*e).into(), acc.into())
                }
            })
            .canonicalize_choice()
        {
            Parser::Choice(left, right) if *left == Parser::Empty => *right,
            e => panic!(
                "Canonization did not return Choice(Empty, canonized) as it was expected. Actual result: {e:?}"
            ),
        };

        options.shuffle(&mut r);

        let second_shuffle = match options
            .iter()
            .fold(Parser::Empty, |acc, e| {
                if r.random_bool(0.5) {
                    Parser::Choice(acc.into(), Parser::Lit(*e).into())
                } else {
                    Parser::Choice(Parser::Lit(*e).into(), acc.into())
                }
            })
            .canonicalize_choice()
        {
            Parser::Choice(left, right) if *left == Parser::Empty => *right,
            e => panic!(
                "Canonization did not return Choice(Empty, canonized) as it was expected. Actual result: {e:?}"
            ),
        };

        assert_eq!(
            first_shuffle, second_shuffle,
            "Randomized Choice did not canonenize correctly."
        );
    }

    #[test]
    fn canonicalize_empty() {
        assert_eq!(
            Parser::Choice(Parser::Empty.into(), Parser::Empty.into()).canonicalize_choice(),
            Parser::Empty
        );
    }

    #[test]
    fn canonicalize_dedup() {
        let mut r = rng();
        let chr: char = r.random();
        let o_chr: char = r.random();
        let mut options = std::iter::once(chr)
            .chain(std::iter::repeat_n(o_chr, r.random_range(3..32)))
            .collect::<Vec<_>>();
        options.shuffle(&mut r);
        let choice = match options
            .iter()
            .fold(Parser::Empty, |acc, e| {
                if r.random_bool(0.5) {
                    Parser::Choice(acc.into(), Parser::Lit(*e).into())
                } else {
                    Parser::Choice(Parser::Lit(*e).into(), acc.into())
                }
            })
            .canonicalize_choice()
        {
            Parser::Choice(left, right) if *left == Parser::Empty => *right,
            e => panic!(
                "Canonization did not return Choice(Empty, canonized) as it was expected. Actual result: {e:?}"
            ),
        };

        assert_eq!(
            choice,
            Parser::Choice(Parser::Lit(chr).into(), Parser::Lit(o_chr).into())
                .canonicalize_choice()
        );
    }

    #[test]
    fn reduce_concatenation() {
        let mut r = rng();
        let chr: char = r.random();

        let min: usize = r.random_range(0..32);

        let mut parts = std::iter::repeat_n(Parser::Lit(chr), min)
            .chain(std::iter::repeat_n(
                Parser::Star(Parser::Lit(chr).into()),
                r.random_range(1..32),
            ))
            .chain(std::iter::repeat_n(Parser::Empty, r.random_range(1..32)))
            .collect::<Vec<_>>();
        parts.shuffle(&mut r);

        let reduced = Parser::reduce_conc(parts);

        let expected = if min > 0 {
            Parser::Conc(
                std::iter::repeat_n(Parser::Lit(chr), min)
                    .chain(std::iter::once(Parser::Star(Parser::Lit(chr).into())))
                    .collect::<Vec<_>>(),
            )
        } else {
            Parser::Star(Parser::Lit(chr).into())
        };

        assert_eq!(reduced, expected);
    }

    #[test]
    fn prefix_factoring_of_identicals() {
        let mut parts = std::iter::from_fn(|| Some(Parser::Lit(rng().random::<char>())))
            .take(rng().random_range(0..32))
            .collect::<Vec<_>>();

        let re = Parser::from_parts(parts.clone());

        parts.push(Parser::Choice(Parser::Empty.into(), Parser::Empty.into()));
        let expected = Parser::from_parts(parts);

        assert_eq!(
            Parser::Choice(re.clone().into(), re.clone().into(),).factor_prefix(),
            expected
        );
    }

    #[test]
    fn prefix_factoring_of_subset_left() {
        let mut prefix = std::iter::from_fn(|| Some(Parser::Lit(rng().random::<char>())))
            .take(rng().random_range(0..32))
            .collect::<Vec<_>>();
        let suffix = std::iter::from_fn(|| Some(Parser::Lit(rng().random::<char>())))
            .take(rng().random_range(0..32))
            .collect::<Vec<_>>();
        let longer = prefix
            .iter()
            .cloned()
            .chain(suffix.iter().cloned())
            .collect::<Vec<_>>();

        let left_long = Parser::Choice(
            Parser::from_parts(longer.clone()).into(),
            Parser::from_parts(prefix.clone()).into(),
        );

        // building `expected`
        if !suffix.is_empty() {
            prefix.push(Parser::Choice(
                Parser::Empty.into(),
                Parser::from_parts(suffix).into(),
            ));
        }
        let expected = Parser::from_parts(prefix);

        assert_eq!(
            left_long.factor_prefix().simplify(),
            expected,
            "Failed to correctly factor prefix if right is prefix of left. Other way not tested."
        );
    }
    #[test]
    fn prefix_factoring_of_subset_right() {
        let mut prefix = std::iter::from_fn(|| Some(Parser::Lit(rng().random::<char>())))
            .take(rng().random_range(0..32))
            .collect::<Vec<_>>();
        let suffix = std::iter::from_fn(|| Some(Parser::Lit(rng().random::<char>())))
            .take(rng().random_range(0..32))
            .collect::<Vec<_>>();
        let longer = prefix
            .iter()
            .cloned()
            .chain(suffix.iter().cloned())
            .collect::<Vec<_>>();

        let right_long = Parser::Choice(
            Parser::from_parts(prefix.clone()).into(),
            Parser::from_parts(longer.clone()).into(),
        );

        // building `expected`
        if !suffix.is_empty() {
            prefix.push(Parser::Choice(
                Parser::Empty.into(),
                Parser::from_parts(suffix).into(),
            ));
        }
        let expected = Parser::from_parts(prefix);

        assert_eq!(
            right_long.factor_prefix().simplify(),
            expected,
            "Failed to correctly factor prefix if right is prefix of left. Other way not tested."
        );
    }

    #[test]
    fn prefix_factoring() {
        let mut prefix = std::iter::from_fn(|| Some(Parser::Lit(rng().random::<char>())))
            .take(rng().random_range(0..32))
            .collect::<Vec<_>>();
        let left_suffix = std::iter::from_fn(|| Some(Parser::Lit(rng().random::<char>())))
            .take(rng().random_range(1..32))
            .collect::<Vec<_>>();
        let right_suffix = {
            loop {
                let candidate = std::iter::from_fn(|| Some(Parser::Lit(rng().random::<char>())))
                    .take(rng().random_range(1..32))
                    .collect::<Vec<_>>();
                if candidate[0] != left_suffix[0] {
                    break candidate;
                }
            }
        };

        let left = Parser::from_parts(
            prefix
                .iter()
                .cloned()
                .chain(left_suffix.iter().cloned())
                .collect::<Vec<_>>(),
        );
        let right = Parser::from_parts(
            prefix
                .iter()
                .cloned()
                .chain(right_suffix.iter().cloned())
                .collect::<Vec<_>>(),
        );
        let choice = Parser::Choice(left.into(), right.into());

        prefix.push(Parser::Choice(
            Parser::from_parts(left_suffix).into(),
            Parser::from_parts(right_suffix).into(),
        ));
        let expected = Parser::from_parts(prefix);

        assert_eq!(
            choice.factor_prefix(),
            expected,
            "Failed to factor common prefix."
        );
    }

    #[test]
    fn factoring_no_prefix() {
        let left = std::iter::from_fn(|| Some(Parser::Lit(rng().random::<char>())))
            .take(rng().random_range(1..32))
            .collect::<Vec<_>>();
        let right = {
            loop {
                let candidate = std::iter::from_fn(|| Some(Parser::Lit(rng().random::<char>())))
                    .take(rng().random_range(1..32))
                    .collect::<Vec<_>>();
                if candidate[0] != left[0] {
                    break candidate;
                }
            }
        };

        let left = Parser::from_parts(left);
        let right = Parser::from_parts(right);
        let choice = Parser::Choice(left.into(), right.into());

        assert_eq!(
            choice.clone().factor_prefix(),
            choice,
            "Failed to factor without common prefix."
        );
    }
}
