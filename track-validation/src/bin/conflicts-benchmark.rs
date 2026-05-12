#![allow(
    clippy::disallowed_macros,
    reason = "this is just a benchmarking binary"
)]
use std::{collections::BTreeMap, str::FromStr};
use track_validation::Regex;

#[derive(PartialOrd, Ord, PartialEq, Eq, Clone, Debug, Copy)]
enum MockType {
    PreparlRegent,
    PreparlEckpup,
    PreparlRegbsl,
    ParlInitiativ,
    ParlVollvlsgn,
    ParlAkzeptanz,
    ParlZurueckgz,
    ParlAblehnung,
    ParlAusschber,
    PostparlKraft,
    PostparlGsblt,
    PreparlVbegde,
    ParlGgentwurf,
    PostparlVesja,
    PostparlVesne,
}
#[derive(Clone, Debug, PartialEq)]
struct MockStat {
    typ: MockType,
    index: u32,
}

fn station_mapping() -> BTreeMap<MockType, char> {
    let mut map = BTreeMap::new();
    map.insert(MockType::PreparlRegent, 'R');
    map.insert(MockType::PreparlEckpup, 'E');
    map.insert(MockType::PreparlRegbsl, 'S');
    map.insert(MockType::ParlInitiativ, 'I');
    map.insert(MockType::ParlVollvlsgn, 'V');
    map.insert(MockType::ParlAkzeptanz, 'J');
    map.insert(MockType::ParlZurueckgz, 'Z');
    map.insert(MockType::ParlAblehnung, 'N');
    map.insert(MockType::ParlAusschber, 'A');
    map.insert(MockType::PostparlKraft, 'K');
    map.insert(MockType::PostparlGsblt, 'G');
    map.insert(MockType::PreparlVbegde, 'B');
    map.insert(MockType::ParlGgentwurf, 'W');
    map.insert(MockType::PostparlVesja, 'Y');
    map.insert(MockType::PostparlVesne, 'X');
    map
}

fn main() {
    let rx =
        Regex::from_str("((E*R+)?S)?I((VA*(Z|VJGK|VN|VA*(Z|VJGK|VN)))|Z)").expect("const string");
    let station_mapping = station_mapping();

    let mut i = 0;
    let stationen: Vec<_> = "IVAAVVVVVJJJJJVVVVVVVVJJJJJJJJVVVVJJJJG"
        .chars()
        .map(|c| {
            i += 1;
            MockStat {
                typ: station_mapping
                    .iter()
                    .find_map(|(k, v)| if *v == c { Some(*k) } else { None })
                    .expect(
                        "Should be findable, because it is a test case and I know what I wrote",
                    ),
                index: i,
            }
        })
        .collect();

    println!("Running");
    let conflicts = rx.conflicting_stations(&stationen, &station_mapping, |s, m| m[&s.typ]);
    println!(
        "Done: {:?}",
        conflicts.iter().map(|s| s.index).collect::<Vec<u32>>()
    );
    assert_eq!(
        conflicts.iter().map(|s| s.index).collect::<Vec<u32>>(),
        [
            5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27,
            28, 29, 30, 31, 32, 35, 36, 37
        ]
    );
}
