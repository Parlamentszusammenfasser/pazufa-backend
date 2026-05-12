use crate::regex::Regex;
use std::collections::{BTreeMap, VecDeque};

impl Regex {
    /// Does BFS for the longest substring (subarray) that satisfies the goal check
    /// if there are multiple substrings of the same length that satisfy this requirement,
    /// it returns the one with the first possible station removed
    /// Example: `ABAABA` for Regex `A*BA*` could either be satisfied by removing the first or second "B":
    /// `AAABA` and `ABAAA` both are valid solutions for this.
    /// this function would return the first `B`
    pub fn conflicting_stations<T: Sized + Clone + PartialEq, ST: Sized, F>(
        &self,
        s: &[T],
        map: &BTreeMap<ST, char>,
        into_char: F,
    ) -> Vec<T>
    where
        F: Fn(&T, &BTreeMap<ST, char>) -> char,
    {
        let mut frontier: VecDeque<(Vec<&T>, usize)> = VecDeque::new();

        frontier.push_back((s.iter().collect(), 0));

        while let Some((current, next_index)) = frontier.pop_front() {
            tracing::trace!(
                "{}-{}, current: \n{:?}",
                current.len(),
                frontier.len(),
                current
                    .iter()
                    .map(|&t| into_char(t, map))
                    .collect::<String>()
            );
            // goal check
            let chars: Vec<_> = current.iter().map(|&t| into_char(t, map)).collect();
            let (b, c) = self.match_regex(&chars);
            tracing::trace!("matched to: {:?}", (b, c));

            if b && c == chars.len() {
                return s.iter().filter(|x| !current.contains(x)).cloned().collect();
            }

            if current.is_empty() {
                return s.into();
            }

            // explanation: c (as in 'consumed') is the longest already matching substring
            for i in next_index..current.len() {
                if i != 0 && into_char(current[i], map) == into_char(current[i - 1], map) {
                    continue;
                }
                let mut subslice: Vec<&T> = current.clone();
                let _ = subslice.remove(i);

                frontier.push_back((subslice, i));
            }
        }
        s.into()
    }
}

#[cfg(test)]
#[allow(
    clippy::arithmetic_side_effects,
    reason = "is either checked in test or can panic"
)]
mod test_regex {
    use crate::Regex;
    use std::{collections::BTreeMap, str::FromStr};

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

    #[test]
    fn conflicting_stations_full_fail() {
        let rx = "[RE]*S?I((VA*(Z|VJGK|VN|VA*(Z|VJGK|VN)))|Z)";
        let rx = Regex::from_str(rx).expect("const string");

        let stats = [MockStat {
            typ: MockType::ParlAusschber,
            index: 0,
        }];

        let csresult = rx.conflicting_stations(&stats, &station_mapping(), |s, m| m[&s.typ]);
        assert_eq!(csresult.len(), 1, "{csresult:?}");
        assert_eq!(csresult[0], stats[0]);
    }
}
