use crate::TrackValidationError;
use std::cmp::Ordering;
use std::collections::VecDeque;
use std::mem;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Regex {
    Star(Box<Regex>),
    Choice(Box<Regex>, Box<Regex>),
    Conc(Vec<Regex>),
    Lit(char),
    Empty,
}

impl PartialOrd for Regex {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(Ord::cmp(&self, &other))
    }
}

impl Ord for Regex {
    fn cmp(&self, other: &Self) -> Ordering {
        let rank = |re: &Regex| match re {
            Regex::Empty => 0,
            Regex::Lit(_) => 1,
            Regex::Star(_) => 2,
            Regex::Conc(_) => 3,
            Regex::Choice(_, _) => 4,
        };

        match (self, other) {
            (Regex::Lit(x), Regex::Lit(y)) => x.cmp(y),
            (Regex::Star(x), Regex::Star(y)) => x.cmp(y),
            (Regex::Conc(xs), Regex::Conc(ys)) => xs.len().cmp(&ys.len()).then_with(|| {
                xs.iter()
                    .zip(ys.iter())
                    .map(|(x, y)| x.cmp(y))
                    .find(|o| *o != Ordering::Equal)
                    .unwrap_or(Ordering::Equal)
            }),
            // Choices should be flattened before we get here,
            // but fall back to structural comparison
            (Regex::Choice(a1, a2), Regex::Choice(b1, b2)) => a1.cmp(b1).then_with(|| a2.cmp(b2)),
            _ => rank(self).cmp(&rank(other)),
        }
    }
}
impl FromStr for Regex {
    type Err = TrackValidationError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let x: Vec<_> = s.chars().collect();
        Self::new(&x)
    }
}

impl Regex {
    pub fn new(rx: &[char]) -> Result<Self, TrackValidationError> {
        if rx.len() == usize::MAX {
            Err(TrackValidationError::RxParsingError)
        } else {
            Self::parse_regex(rx)
        }
    }

    // takes `[ABCDEF]` returns Regex("A|B|C|D|E|F")
    #[allow(
        clippy::arithmetic_side_effects,
        reason = "only overflows for re.len() == usize::MAX, which is asserted out before"
    )]
    fn parse_choicebox(re: &[char]) -> (Regex, usize) {
        let mut last_regex = Regex::Empty;
        let mut ci = 0;

        while ci < re.len() {
            let c = re[ci];
            match (c, &mut last_regex) {
                ('[', _) => {}
                (']', _) => break,
                ('A'..='Z', Regex::Empty) => last_regex = Regex::Lit(c),
                ('A'..='Z', Regex::Lit(_)) => {
                    last_regex = Regex::Choice(Regex::Lit(c).into(), last_regex.into());
                }
                ('A'..='Z', Regex::Choice(_, _)) => {
                    last_regex = Regex::Choice(last_regex.into(), Regex::Lit(c).into());
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
    fn parse_regex(re: &[char]) -> Result<Regex, TrackValidationError> {
        if re.is_empty() {
            return Ok(Regex::Empty);
        }

        let mut rx = Regex::Conc(vec![]);
        let mut ci = 0;

        while ci < re.len() {
            let c = re[ci];
            match (c, &mut rx) {
                ('A'..='Z', Regex::Conc(child)) => child.push(Regex::Lit(c)),
                ('*', Regex::Conc(child)) => {
                    if let Some(last_child) = child.pop() {
                        child.push(Regex::Star(last_child.into()));
                    } else {
                        return Err(TrackValidationError::RxParsingError);
                    }
                }
                ('|', Regex::Conc(_)) => {
                    let rx_slimmed = match rx {
                        Regex::Conc(v) => Regex::from_parts(v),
                        _ => rx.clone(),
                    };
                    rx = Regex::Choice(rx_slimmed.into(), Self::parse_regex(&re[ci + 1..])?.into());
                    ci = re.len();
                    continue;
                }
                ('(', Regex::Conc(child)) => {
                    let subre = Self::find_matching_brace(&re[ci..]);
                    tracing::trace!("matching brace: {:?}", subre);
                    ci += subre.len() + 1;
                    child.push(Self::parse_regex(subre)?);
                }
                ('[', Regex::Conc(child)) => {
                    let (cbrx, cblen) = Self::parse_choicebox(re);
                    ci += cblen - 1;
                    child.push(cbrx);
                }
                ('?', Regex::Conc(child)) => {
                    if let Some(last_child) = child.pop() {
                        child.push(Regex::Choice(last_child.into(), Regex::Empty.into()));
                    } else {
                        return Err(TrackValidationError::RxParsingError);
                    }
                }
                ('+', Regex::Conc(child)) => {
                    if let Some(last_child) = child.last() {
                        child.push(Regex::Star(Box::new(last_child.clone())));
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
            Regex::Conc(v) => Regex::from_parts(v),
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
    fn find_matching_brace(re: &[char]) -> &[char] {
        let mut brace_indent = 0;
        for ci in 0..re.len() {
            match re[ci] {
                '(' => brace_indent += 1,
                ')' => brace_indent -= 1,
                _ => {}
            }
            if brace_indent == 0 {
                return &re[1..ci];
            }
        }
        &re[1..re.len().checked_sub(1).expect("expression is malformed")]
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
            Self::Star(inner) if *inner == Regex::Empty => Regex::Empty,
            Self::Star(inner) if matches!(*inner, Regex::Star(_)) => *inner,
            Self::Choice(left, right) if left == right => *left,
            Self::Choice(left, right)
                if *left == Regex::Empty && matches!(*right, Regex::Star(_)) =>
            {
                *right
            }
            Self::Choice(left, right)
                if *right == Regex::Empty && matches!(*left, Regex::Star(_)) =>
            {
                *left
            }
            Self::Choice(left, right)
                if matches!(*right, Regex::Star(_)) && {
                    if let Regex::Star(inner) = &*right {
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

            Regex::Conc(inner) => Self::reduce_conc(inner),
            other => other,
        }
    }

    fn from_parts(mut parts: Vec<Regex>) -> Regex {
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
    fn canonicalize_choice(self) -> Regex {
        fn flatten_choice(re: Regex) -> Vec<Regex> {
            match re {
                Regex::Choice(left, right) => {
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
            .unwrap_or(Regex::Empty)
    }

    fn factor_prefix(self) -> Regex {
        if let Self::Choice(left, right) = self {
            let left: &mut dyn Iterator<Item = Regex> = if let Self::Conc(left) = *left {
                &mut left.into_iter()
            } else {
                &mut std::iter::once(*left)
            };
            let right: &mut dyn Iterator<Item = Regex> = if let Self::Conc(right) = *right {
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
                    if left == right && left != Regex::Empty {
                        prefix.push(left);
                    } else {
                        new_left.push(left);
                        new_right.push(right);
                        break;
                    }
                } else if left == Regex::Empty {
                    continue;
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
    fn reduce_conc(parts: Vec<Regex>) -> Regex {
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

        let parts = std::iter::repeat_n(Regex::Star(Regex::Lit(chr).into()), 1)
            .chain(std::iter::repeat_n(Regex::Empty, 1))
            .collect::<Vec<_>>();

        let reduced = Regex::reduce_conc(parts);

        let expected = Regex::Star(Regex::Lit(chr).into());

        assert_eq!(reduced, expected);
    }

    #[test]
    fn issue44_empty_regex() {
        let mut parts = vec![];

        let re = Regex::from_parts(parts.clone());

        parts.push(Regex::Choice(Regex::Empty.into(), Regex::Empty.into()));
        let expected = Regex::from_parts(parts);

        assert_eq!(
            Regex::Choice(re.clone().into(), re.clone().into(),).factor_prefix(),
            expected
        );
    }
    #[test]
    fn issue45_empty_suffix() {
        let pool = vec![Regex::Lit('\u{623cd}')];
        let prefix = pool.into_iter().take(1).collect::<Vec<_>>();
        // This is the interesting part: suffix empty means the reduction is one level up
        let suffix = [];
        let longer = prefix
            .iter()
            .cloned()
            .chain(suffix.iter().cloned())
            .collect::<Vec<_>>();

        let left_long = Regex::Choice(
            Regex::from_parts(longer.clone()).into(),
            Regex::from_parts(prefix.clone()).into(),
        );
        let right_long = Regex::Choice(
            Regex::from_parts(prefix.clone()).into(),
            Regex::from_parts(longer).into(),
        );

        let expected = Regex::from_parts(prefix);

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
        let pool = "abop".chars().map(Regex::Lit);
        let mut prefix = pool.clone().take(0).collect::<Vec<_>>();
        let suffix = pool.rev().take(3).collect::<Vec<_>>();
        let longer = prefix
            .iter()
            .cloned()
            .chain(suffix.iter().cloned())
            .collect::<Vec<_>>();

        let left_long = Regex::Choice(
            Regex::from_parts(longer.clone()).into(),
            Regex::from_parts(prefix.clone()).into(),
        );

        // building `expected`
        if !suffix.is_empty() {
            prefix.push(Regex::Choice(
                Regex::Empty.into(),
                Regex::from_parts(suffix).into(),
            ));
        };
        let expected = Regex::from_parts(prefix);

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
        let output = Regex::Conc(vec![Regex::Lit('A'), Regex::Lit('B'), Regex::Lit('C')]);
        assert_eq!(output, Regex::parse_regex(&input).expect("const input"));
    }

    #[test]
    fn star_on_char() {
        let input = "BA*";
        let input: Vec<char> = input.chars().collect();
        let output = Regex::Conc(vec![
            Regex::Lit('B'),
            Regex::Star(Box::new(Regex::Lit('A'))),
        ]);
        assert_eq!(output, Regex::parse_regex(&input).expect("const input"));
    }

    #[test]
    fn choice_concat() {
        let input = "BB|AA";
        let input: Vec<char> = input.chars().collect();
        let output = Regex::Choice(
            Regex::Conc(vec![Regex::Lit('A'), Regex::Lit('A')]).into(),
            Regex::Conc(vec![Regex::Lit('B'), Regex::Lit('B')]).into(),
        );
        assert_eq!(output, Regex::parse_regex(&input).expect("const input"));
    }

    #[test]
    fn choice() {
        let input = "B|A";
        let input: Vec<char> = input.chars().collect();
        let output = Regex::Choice(Box::new(Regex::Lit('A')), Box::new(Regex::Lit('B')));
        assert_eq!(output, Regex::parse_regex(&input).expect("const input"));
    }

    #[test]
    fn choice_braces() {
        let input = "B|(AB)";
        let input: Vec<char> = input.chars().collect();
        let output = Regex::Choice(
            Box::new(Regex::Lit('B')),
            Box::new(Regex::Conc(vec![Regex::Lit('A'), Regex::Lit('B')])),
        );
        assert_eq!(output, Regex::parse_regex(&input).expect("const input"));
    }

    #[test]
    fn brace_then_star() {
        let input = "(AB)*";
        let input: Vec<char> = input.chars().collect();
        let output = Regex::Star(Regex::Conc(vec![Regex::Lit('A'), Regex::Lit('B')]).into());
        assert_eq!(output, Regex::parse_regex(&input).expect("const input"));
    }

    #[test]
    fn choicebox_parsing() {
        let input = "[ABC]";
        let input: Vec<char> = input.chars().collect();
        let output = Regex::Choice(
            Regex::Choice(Regex::Lit('B').into(), Regex::Lit('A').into()).into(),
            Regex::Lit('C').into(),
        );

        assert_eq!((output, 5), Regex::parse_choicebox(&input));
    }
    #[test]
    fn choiceboxstar() {
        let input = "[ABC]*";
        let input: Vec<char> = input.chars().collect();
        let output = Regex::Star(
            Regex::Choice(
                Regex::Lit('A').into(),
                Regex::Choice(Regex::Lit('B').into(), Regex::Lit('C').into()).into(),
            )
            .into(),
        );
        assert_eq!(output, Regex::parse_regex(&input).expect("const input"));
    }

    #[test]
    fn questionmark() {
        let input = "A?";
        let input: Vec<char> = input.chars().collect();
        let output = Regex::Choice(Regex::Empty.into(), Regex::Lit('A').into());
        assert_eq!(output, Regex::parse_regex(&input).expect("const input"));
    }
    #[test]
    fn plus() {
        let input = "A+";
        let input: Vec<char> = input.chars().collect();
        let output = Regex::Conc(vec![Regex::Lit('A'), Regex::Star(Regex::Lit('A').into())]);
        assert_eq!(output, Regex::parse_regex(&input).expect("const input"));
    }

    #[test]
    fn test_canonicalizes_choicebox() {
        let mut r = rng();
        let mut options = ['A', 'B', 'C', 'D', 'E', 'F'];
        options.shuffle(&mut r);

        let randomized_choice = match options
            .iter()
            .fold(Regex::Empty, |acc, e| {
                if r.random_bool(0.5) {
                    Regex::Choice(acc.into(), Regex::Lit(*e).into())
                } else {
                    Regex::Choice(Regex::Lit(*e).into(), acc.into())
                }
            })
            .canonicalize_choice()
        {
            Regex::Choice(left, right) if *left == Regex::Empty => *right,
            e => panic!(
                "Canonization did not return Choice(Empty, canonized) as it was expected. Actual result: {e:?}"
            ),
        };

        let parsed_choice =
            Regex::new(&"[ABCDEF]".chars().collect::<Vec<_>>()).expect("const input");

        let expected = Regex::Choice(
            Regex::Lit('A').into(),
            Regex::Choice(
                Regex::Lit('B').into(),
                Regex::Choice(
                    Regex::Lit('C').into(),
                    Regex::Choice(
                        Regex::Lit('D').into(),
                        Regex::Choice(Regex::Lit('E').into(), Regex::Lit('F').into()).into(),
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
            .fold(Regex::Empty, |acc, e| {
                if r.random_bool(0.5) {
                    Regex::Choice(acc.into(), Regex::Lit(*e).into())
                } else {
                    Regex::Choice(Regex::Lit(*e).into(), acc.into())
                }
            })
            .canonicalize_choice()
        {
            Regex::Choice(left, right) if *left == Regex::Empty => *right,
            e => panic!(
                "Canonization did not return Choice(Empty, canonized) as it was expected. Actual result: {e:?}"
            ),
        };

        options.shuffle(&mut r);

        let second_shuffle = match options
            .iter()
            .fold(Regex::Empty, |acc, e| {
                if r.random_bool(0.5) {
                    Regex::Choice(acc.into(), Regex::Lit(*e).into())
                } else {
                    Regex::Choice(Regex::Lit(*e).into(), acc.into())
                }
            })
            .canonicalize_choice()
        {
            Regex::Choice(left, right) if *left == Regex::Empty => *right,
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
            Regex::Choice(Regex::Empty.into(), Regex::Empty.into()).canonicalize_choice(),
            Regex::Empty
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
            .fold(Regex::Empty, |acc, e| {
                if r.random_bool(0.5) {
                    Regex::Choice(acc.into(), Regex::Lit(*e).into())
                } else {
                    Regex::Choice(Regex::Lit(*e).into(), acc.into())
                }
            })
            .canonicalize_choice()
        {
            Regex::Choice(left, right) if *left == Regex::Empty => *right,
            e => panic!(
                "Canonization did not return Choice(Empty, canonized) as it was expected. Actual result: {e:?}"
            ),
        };

        assert_eq!(
            choice,
            Regex::Choice(Regex::Lit(chr).into(), Regex::Lit(o_chr).into()).canonicalize_choice()
        );
    }

    #[test]
    fn reduce_concatenation() {
        let mut r = rng();
        let chr: char = r.random();

        let min: usize = r.random_range(0..32);

        let mut parts = std::iter::repeat_n(Regex::Lit(chr), min)
            .chain(std::iter::repeat_n(
                Regex::Star(Regex::Lit(chr).into()),
                r.random_range(1..32),
            ))
            .chain(std::iter::repeat_n(Regex::Empty, r.random_range(1..32)))
            .collect::<Vec<_>>();
        parts.shuffle(&mut r);

        let reduced = Regex::reduce_conc(parts);

        let expected = if min > 0 {
            Regex::Conc(
                std::iter::repeat_n(Regex::Lit(chr), min)
                    .chain(std::iter::once(Regex::Star(Regex::Lit(chr).into())))
                    .collect::<Vec<_>>(),
            )
        } else {
            Regex::Star(Regex::Lit(chr).into())
        };

        assert_eq!(reduced, expected);
    }

    #[test]
    fn prefix_factoring_of_identicals() {
        let mut parts = std::iter::from_fn(|| Some(Regex::Lit(rng().random::<char>())))
            .take(rng().random_range(0..32))
            .collect::<Vec<_>>();

        let re = Regex::from_parts(parts.clone());

        parts.push(Regex::Choice(Regex::Empty.into(), Regex::Empty.into()));
        let expected = Regex::from_parts(parts);

        assert_eq!(
            Regex::Choice(re.clone().into(), re.clone().into(),).factor_prefix(),
            expected
        );
    }

    #[test]
    fn prefix_factoring_of_subset_left() {
        let mut prefix = std::iter::from_fn(|| Some(Regex::Lit(rng().random::<char>())))
            .take(rng().random_range(0..32))
            .collect::<Vec<_>>();
        let suffix = std::iter::from_fn(|| Some(Regex::Lit(rng().random::<char>())))
            .take(rng().random_range(0..32))
            .collect::<Vec<_>>();
        let longer = prefix
            .iter()
            .cloned()
            .chain(suffix.iter().cloned())
            .collect::<Vec<_>>();

        let left_long = Regex::Choice(
            Regex::from_parts(longer.clone()).into(),
            Regex::from_parts(prefix.clone()).into(),
        );

        // building `expected`
        if !suffix.is_empty() {
            prefix.push(Regex::Choice(
                Regex::Empty.into(),
                Regex::from_parts(suffix).into(),
            ));
        };
        let expected = Regex::from_parts(prefix);

        assert_eq!(
            left_long.factor_prefix().simplify(),
            expected,
            "Failed to correctly factor prefix if right is prefix of left. Other way not tested."
        );
    }
    #[test]
    fn prefix_factoring_of_subset_right() {
        let mut prefix = std::iter::from_fn(|| Some(Regex::Lit(rng().random::<char>())))
            .take(rng().random_range(0..32))
            .collect::<Vec<_>>();
        let suffix = std::iter::from_fn(|| Some(Regex::Lit(rng().random::<char>())))
            .take(rng().random_range(0..32))
            .collect::<Vec<_>>();
        let longer = prefix
            .iter()
            .cloned()
            .chain(suffix.iter().cloned())
            .collect::<Vec<_>>();

        let right_long = Regex::Choice(
            Regex::from_parts(prefix.clone()).into(),
            Regex::from_parts(longer.clone()).into(),
        );

        // building `expected`
        if !suffix.is_empty() {
            prefix.push(Regex::Choice(
                Regex::Empty.into(),
                Regex::from_parts(suffix).into(),
            ));
        };
        let expected = Regex::from_parts(prefix);

        assert_eq!(
            right_long.factor_prefix().simplify(),
            expected,
            "Failed to correctly factor prefix if right is prefix of left. Other way not tested."
        );
    }

    #[test]
    fn prefix_factoring() {
        let mut prefix = std::iter::from_fn(|| Some(Regex::Lit(rng().random::<char>())))
            .take(rng().random_range(0..32))
            .collect::<Vec<_>>();
        let left_suffix = std::iter::from_fn(|| Some(Regex::Lit(rng().random::<char>())))
            .take(rng().random_range(1..32))
            .collect::<Vec<_>>();
        let right_suffix = {
            loop {
                let candidate = std::iter::from_fn(|| Some(Regex::Lit(rng().random::<char>())))
                    .take(rng().random_range(1..32))
                    .collect::<Vec<_>>();
                if candidate[0] != left_suffix[0] {
                    break candidate;
                }
            }
        };

        let left = Regex::from_parts(
            prefix
                .iter()
                .cloned()
                .chain(left_suffix.iter().cloned())
                .collect::<Vec<_>>(),
        );
        let right = Regex::from_parts(
            prefix
                .iter()
                .cloned()
                .chain(right_suffix.iter().cloned())
                .collect::<Vec<_>>(),
        );
        let choice = Regex::Choice(left.into(), right.into());

        prefix.push(Regex::Choice(
            Regex::from_parts(left_suffix).into(),
            Regex::from_parts(right_suffix).into(),
        ));
        let expected = Regex::from_parts(prefix);

        assert_eq!(
            choice.factor_prefix(),
            expected,
            "Failed to factor common prefix."
        );
    }

    #[test]
    fn factoring_no_prefix() {
        let left = std::iter::from_fn(|| Some(Regex::Lit(rng().random::<char>())))
            .take(rng().random_range(1..32))
            .collect::<Vec<_>>();
        let right = {
            loop {
                let candidate = std::iter::from_fn(|| Some(Regex::Lit(rng().random::<char>())))
                    .take(rng().random_range(1..32))
                    .collect::<Vec<_>>();
                if candidate[0] != left[0] {
                    break candidate;
                }
            }
        };

        let left = Regex::from_parts(left);
        let right = Regex::from_parts(right);
        let choice = Regex::Choice(left.into(), right.into());

        assert_eq!(
            choice.clone().factor_prefix(),
            choice,
            "Failed to factor without common prefix."
        );
    }
}
