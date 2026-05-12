use crate::regex::Regex;
fn match_conc(input: &[char], children: &[Regex]) -> (bool, usize) {
    match children {
        [head, tail @ ..] => match head.match_regex(input) {
            (true, match_len) => {
                let (valid, child_match_len) = match_conc(&input[match_len..], tail);
                (
                    valid,
                    match_len.checked_add(child_match_len).unwrap_or_else(|| {
                        panic!("Failed to match regex: input length exceeded usize::MAX")
                    }),
                )
            }
            result => result,
        },
        [] => (true, 0),
    }
}
impl Regex {
    // rx('aa*aa') // matches anything from 3 to n 'a'
    // aaaaaa (length n)
    // star means: anything from 0 to n times matches the expression
    // returns: (valid, consumed_characters) which also means consumed_chars is the length of the
    // longest matching substring
    #[must_use]
    pub fn match_regex(&self, input: &[char]) -> (bool, usize) {
        if input.is_empty() {
            return (true, 0);
        }
        match self {
            Regex::Lit(x) => (*x == input[0], usize::from(*x == input[0])),
            Regex::Choice(ra, rb) => {
                let (ba, ca) = ra.match_regex(input);
                let (bb, cb) = rb.match_regex(input);
                if ba && bb {
                    (true, ca.max(cb))
                } else if ba {
                    (true, ca)
                } else if bb {
                    (true, cb)
                } else {
                    (false, ca.max(cb))
                }
            }
            Regex::Conc(children) => match_conc(input, children),
            #[allow(
                clippy::arithmetic_side_effects,
                reason = "cannot exceed length of string"
            )]
            Regex::Star(rx) => {
                let mut count = 0;
                while !input[count..].is_empty() {
                    let (b, cnt) = rx.match_regex(&input[count..]);
                    if !b {
                        break;
                    }

                    count += cnt;
                }
                (true, count)
            }
            Regex::Empty => (true, 0),
        }
    }
}
#[cfg(test)]
#[allow(
    clippy::arithmetic_side_effects,
    reason = "is either checked in test or can panic"
)]
mod test {
    use crate::Regex;
    use std::str::FromStr;
    use tracing_test::traced_test;

    #[test]
    #[traced_test]
    fn match_rx_endless_loop() {
        let rx =
            Regex::from_str("[RE]*S?I((VA*(Z|VJGK|VN|VA*(Z|VJGK|VN)))|Z)").expect("const string");
        let to_match = ['R'];
        assert!(matches!(rx.match_regex(&to_match), (true, x) if x == 1));
    }
    #[test]
    fn match_rx_fail() {
        let rx = "[RE]*S?I((VA*(Z|VJGK|VN|VA*(Z|VJGK|VN)))|Z)";
        let rx = Regex::from_str(rx).expect("const string");
        let strong = ['A'];
        let (b, c) = rx.match_regex(&strong);
        assert!(
            matches!((b, c), (false, 0)),
            "1 / {strong:?}\n || \n ({b};{c})"
        );
    }

    fn strrxmatch(rx: &str, st: &str) -> (bool, usize) {
        let vrx: Vec<_> = rx.chars().collect();
        let vst: Vec<_> = st.chars().collect();
        Regex::new(&vrx)
            .expect("test case setup failed")
            .match_regex(&vst)
    }

    #[test]
    fn matchtest_one() {
        let rx = "A";
        // match if exact match
        let string = "A";
        assert!(strrxmatch(rx, string).0);
        // match and .1 < len
        let string2 = "AA";
        let result = strrxmatch(rx, string2);
        assert!(result.0 && result.1 < string2.len());
        // partial match
        let rx2 = "AA";
        assert!(strrxmatch(rx2, string).0);
    }

    #[test]
    fn matchtest_parlament() {
        let rx = "[RE]*S?I((VA*(Z|VJGK|VN|(VA*Z|VA*VJGK|VA*VN)))|Z)";

        let st = "RSIVZ";
        assert!(strrxmatch(rx, st).0);

        let st = "RZ";
        assert!(!strrxmatch(rx, st).0);

        let st = "RIZA";
        let result = strrxmatch(rx, st);
        assert!(result.0);
        assert!(result.1 < st.len());
    }
}
