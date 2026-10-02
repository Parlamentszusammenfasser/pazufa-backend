use crate::parser::Parser;
use std::fmt::Display;

impl Parser {
    fn print(&self) -> String {
        match self {
            Self::Empty => String::new(),
            Self::Lit(c) => c.to_string(),
            Self::Star(rx) => match &**rx {
                Self::Lit(x) => format!("{x}*"),
                other => format!("({})*", other.print()),
            },
            Self::Conc(rx) => rx.iter().fold(String::new(), |a, b| format!("{a}{b}")),
            Self::Choice(rxa, rxb) => format!("{}|{}", rxa.print(), rxb.print()),
        }
    }
}
impl Display for Parser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.print())
    }
}
#[cfg(test)]
#[allow(
    clippy::arithmetic_side_effects,
    reason = "is either checked in test or can panic"
)]
mod test {
    use crate::Regex;
    use tracing_test::traced_test;

    #[test]
    #[traced_test]
    fn match_issue207_case() {
        let rx = Regex::new("(LA*)+LJ").expect("const str");
        let to_match = "LLJ";
        let nfa_match = rx.matches(to_match);
        assert!(matches!(nfa_match, (true, 3)));
        // let r = rx.matches(&to_match);
        // assert!(matches!(r, (true, x) if x == 3), "got: {r:?}");
    }

    #[test]
    #[traced_test]
    fn match_rx_endless_loop() {
        let rx = Regex::new("[RE]*S?I((VA*(Z|VJGK|VN|VA*(Z|VJGK|VN)))|Z)").expect("const string");
        let to_match = "R";
        assert!(matches!(rx.matches(to_match), (true, x) if x == 1));
    }

    #[test]
    fn match_rx_fail() {
        let rx = "[RE]*S?I((VA*(Z|VJGK|VN|VA*(Z|VJGK|VN)))|Z)";
        let rx = Regex::new(rx).expect("const string");
        let strong = "A";
        let (b, c) = rx.matches(strong);
        assert!(
            matches!((b, c), (false, 0)),
            "1 / {strong:?}\n || \n ({b};{c})"
        );
    }

    fn strrxmatch(rx: &str, st: &str) -> (bool, usize) {
        let rx = Regex::new(rx).expect("test case setup failed");
        rx.matches(st)
    }

    #[test]
    #[traced_test]
    fn matchtest_one() {
        let rx = "A";
        // match if exact match
        let string = "A";
        assert!(strrxmatch(rx, string).0);
        // match and .1 == 1 (=one consumed)
        let string2 = "AA";
        let result = strrxmatch(rx, string2);
        assert!(!result.0);
        assert_eq!(result.1, 1);
        // partial match
        let rx2 = "AA";
        assert!(strrxmatch(rx2, string).0);
    }

    #[test]
    #[traced_test]
    fn matchtest_parlament() {
        let rx = "[RE]*S?I((VA*(Z|VJGK|VN|(VA*Z|VA*VJGK|VA*VN)))|Z)";

        // checks valid full path
        let st = "RSIVZ";
        assert!(strrxmatch(rx, st).0);

        // checks failure on wrong path (there is no R->Z transition)
        let st = "RZ";
        assert!(!strrxmatch(rx, st).0);

        // checks input hangover, which is a failure
        let st = "RIZA";
        let result = strrxmatch(rx, st);
        assert!(!result.0, "{result:?}");
        assert_eq!(result.1, 3);
    }
}
