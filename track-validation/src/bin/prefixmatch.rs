#![allow(clippy::expect_used, reason = "will panic anyway, is just a small exe")]
#![allow(
    clippy::disallowed_macros,
    reason = "I do not need a full tracing stack"
)]
use track_validation::Regex;

const USAGE: &str = "USAGE: `prefixmatch <regex> <input string>`";
const BACKEND_REJ: &str = "The backend would reject this";
const BACKEND_ACC: &str = "The backend would accept this";

fn main() {
    let mut args = std::env::args();
    assert_eq!(args.len(), 3, "Wrong number of Arguments: {USAGE}");

    let regex = args.nth(1).expect(USAGE);
    let input = args.next().expect(USAGE);
    let regex = Regex::new(&regex).expect("Regex could not be parsed");
    let (matched, consumed) = regex.matches(&input);
    let length = input.len();

    match (matched, consumed) {
        (true, consumed) => match consumed.cmp(&length) {
            std::cmp::Ordering::Less => {
                unreachable!(
                    "State matches but input is left over, this is a bug, please report it"
                );
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
            if consumed < input.len() {
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
            } else {
                println!(
                    "Not Matched! {consumed}/{} input characters consumed",
                    input.len()
                );
                println!("Input is Left Over");
                println!("{BACKEND_REJ}");
            }
            std::process::exit(1)
        }
    }
}
