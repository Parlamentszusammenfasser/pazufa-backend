#![allow(clippy::expect_used, reason = "will panic anyway, is just a small exe")]
#![allow(
    clippy::disallowed_macros,
    reason = "I do not need a full tracing stack"
)]
use std::process::exit;
use track_validation::Regex;

const USAGE: &str = "USAGE: `prefixmatch <regex> <input string>`";
const BACKEND_REJ: &str = "The backend would reject this";
const BACKEND_ACC: &str = "The backend would accept this";

fn main() {
    let mut args = std::env::args();
    assert_eq!(args.len(), 3, "Wrong number of Arguments: {USAGE}");

    let regex = args.nth(1).expect(USAGE);
    let input = args.next().expect(USAGE);
    let regex: Vec<_> = regex.chars().collect();
    let regex = Regex::new(&regex).expect("Regex could not be parsed");
    let input: Vec<_> = input.chars().collect();
    let (matched, consumed) = regex.match_regex(&input);
    let length = input.len();

    match (matched, consumed) {
        (true, consumed) => match consumed.cmp(&length) {
            std::cmp::Ordering::Less => {
                println!(
                    "Partially Matched! {consumed}/{} input characters consumed, meaning some input is left over:",
                    input.len()
                );
                println!(
                    "{:?} matched / {:?} left over",
                    &input[..consumed],
                    &input[consumed..]
                );
                println!("{BACKEND_REJ}");
                exit(1);
            }
            std::cmp::Ordering::Equal => {
                println!("Matched, input fully consumed");
                println!("{BACKEND_ACC}");
            }
            std::cmp::Ordering::Greater => {
                println!(
                    "Matched! {consumed}/{} input characters consumed",
                    input.len()
                );
                println!("{BACKEND_ACC}");
            }
        },
        (false, consumed) => {
            println!(
                "Not Matched! {consumed}/{} input characters consumed",
                input.len()
            );
            println!(
                "{:?} matched / {:?} failed the match",
                &input[..consumed],
                &input[consumed..]
            );
            println!("{BACKEND_REJ}");
        }
    }
}
