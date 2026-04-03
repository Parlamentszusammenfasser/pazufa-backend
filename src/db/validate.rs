use crate::{LTZFServer, Result, error::DataValidationError};
use openapi::models;
use serde::Deserialize;
use smallvec::SmallVec;
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

#[derive(Deserialize)]
struct RawTracks {
    version: String,
    stations: BTreeMap<char, models::Stationstyp>,
    tracks: BTreeMap<models::Parlament, BTreeMap<models::Vorgangstyp, String>>,
}

pub struct Tracks {
    pub version: String,
    pub station_mapping: BTreeMap<models::Stationstyp, char>,
    pub tracks: BTreeMap<(models::Parlament, models::Vorgangstyp), Regex>,
}
impl Tracks {
    pub fn new(path: &str) -> Self {
        let file = std::fs::read_to_string(std::path::Path::new(path)).unwrap_or_else(|error| {
            panic!("Expected to find a tracks file configured as: `{path}`. Error: {error}")
        });
        let rt: RawTracks = toml::from_str(&file).unwrap();
        let tracks: BTreeMap<_, Regex> = rt
            .tracks
            .iter()
            .flat_map(|(parl, v)| {
                v.iter().map(|(&vtyp, rx)| {
                    ((*parl, vtyp), {
                        tracing::trace!("parsing {rx}");
                        Regex::new(rx)
                    })
                })
            })
            .collect();
        let sm: BTreeMap<_, char> = rt.stations.iter().map(|(k, v)| (*v, *k)).collect();
        Self {
            version: rt.version,
            station_mapping: sm,
            tracks,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum Regex {
    Star(Box<Regex>),
    Choice(Box<Regex>, Box<Regex>),
    Conc(Vec<Regex>),
    Lit(char),
    Empty,
}

impl Regex {
    pub fn new(re: &str) -> Regex {
        let x: Vec<_> = re.chars().collect();
        Self::parse_regex(&x)
    }

    // takes `[ABCDEF]` returns Regex("A|B|C|D|E|F")
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
                    last_regex = Regex::Choice(Regex::Lit(c).into(), last_regex.into())
                }
                ('A'..='Z', Regex::Choice(_, _)) => {
                    last_regex = Regex::Choice(last_regex.into(), Regex::Lit(c).into())
                }
                _ => panic!(),
            }
            ci += 1;
        }
        (last_regex, ci + 1)
    }

    // LIMITATION: parses a*aa into a*aa, and does no reordering.
    // This assumes that stars are always at the end of expressions of the same kind:
    // a*aa <=> aaa*
    fn parse_regex(re: &[char]) -> Regex {
        if re.is_empty() {
            return Regex::Empty;
        }

        let mut rx = Regex::Conc(vec![]);
        let mut ci = 0;

        while ci < re.len() {
            let c = re[ci];
            match (c, &mut rx) {
                ('A'..='Z', Regex::Conc(child)) => child.push(Regex::Lit(c)),
                ('*', Regex::Conc(child)) => {
                    let last_child = child.pop().unwrap();
                    child.push(Regex::Star(last_child.into()));
                }
                ('|', Regex::Conc(_)) => {
                    let rx_slimmed = match rx {
                        Regex::Conc(v) => {
                            if v.is_empty() {
                                Regex::Empty
                            } else if v.len() == 1 {
                                v[0].clone()
                            } else {
                                Regex::Conc(v)
                            }
                        }
                        _ => rx.clone(),
                    };
                    rx = Regex::Choice(rx_slimmed.into(), Self::parse_regex(&re[ci + 1..]).into());
                    ci = re.len();
                    continue;
                }
                ('(', Regex::Conc(child)) => {
                    let subre = Self::find_matching_brace(&re[ci..]);
                    tracing::trace!("matching brace: {:?}", subre);
                    ci += subre.len() + 1;
                    child.push(Self::parse_regex(subre));
                }
                ('[', Regex::Conc(child)) => {
                    let (cbrx, cblen) = Self::parse_choicebox(re);
                    ci += cblen - 1;
                    child.push(cbrx);
                }
                ('?', Regex::Conc(child)) => {
                    let last_child = child.pop().unwrap();
                    child.push(Regex::Choice(last_child.into(), Regex::Empty.into()));
                }
                ('+', Regex::Conc(child)) => {
                    let last_child = child.last().unwrap().clone();
                    child.push(Regex::Star(Box::new(last_child)));
                }
                _ => panic!("for {:?}: {} @ {}", re, c, ci),
            }
            ci += 1;
        }

        match rx {
            Regex::Conc(v) => {
                if v.is_empty() {
                    Regex::Empty
                } else if v.len() == 1 {
                    v[0].clone()
                } else {
                    Regex::Conc(v)
                }
            }
            _ => rx,
        }
        // TODO: make canonical representation of a
        // reorder A*AA to AAA*, make conc('A') to 'A' etc
    }

    // takes `(abc(de)fg(h))` and returns `abc(de)fg(h)`
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
        &re[1..re.len() - 1]
    }
    // rx('aa*aa') // matches anything from 3 to n 'a'
    // aaaaaa (length n)
    // star means: anything from 0 to n times matches the expression
    // returns: (valid, consumed_characters) which also means consumed_chars is the length of the
    // longest matching substring
    pub fn match_regex(&self, input: &[char]) -> (bool, usize) {
        if input.is_empty() {
            return (true, 0);
        }
        match self {
            Regex::Lit(x) => (*x == input[0], if *x == input[0] { 1 } else { 0 }),
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
            Regex::Conc(children) => {
                if children.is_empty() {
                    return (true, 0);
                }
                let (b, cons) = children.first().unwrap().match_regex(input);
                if !b {
                    return (false, cons);
                }
                let tailchildren = children.iter().skip(1).cloned().collect();
                let (tb, tc) = Regex::Conc(tailchildren).match_regex(&input[cons..]);
                if !tb {
                    return (false, cons + tc);
                }
                (true, tc + cons)
            }
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

    /// Does BFS for the longest substring (subarray) that satisfies the goal check
    /// if there are multiple substrings of the same length that satisfy this requirement,
    /// it returns the one with the first possible station removed
    /// Example: `ABAABA` for Regex `A*BA*` could either be satisfied by removing the first or second "B":
    /// `AAABA` and `ABAAA` both are valid solutions for this.
    /// this function would return the first `B`
    fn conflicting_stations(
        &self,
        s: &[models::Station],
        map: &BTreeMap<models::Stationstyp, char>,
    ) -> Vec<models::Station> {
        let mut frontier: VecDeque<SmallVec<[usize; 32]>> = VecDeque::new();
        let mut seen = HashSet::new();

        frontier.push_back((0usize..s.len()).collect());
        while !frontier.is_empty() {
            let current = frontier
                .pop_front()
                .expect("must be possible since frontier.is_empty was false");

            tracing::trace!(
                "{}-{}-{}, current: \n{:?}",
                current.len(),
                seen.len(),
                frontier.len(),
                current.iter().map(|&i| map[&s[i].typ]).collect::<String>()
            );
            // goal check
            let chars: Vec<_> = current.iter().map(|&x| map[&s[x].typ]).collect();
            let (b, c) = self.match_regex(&chars);
            tracing::trace!("matched to: {:?}", (b, c));

            if b && c == chars.len() {
                return s
                    .iter()
                    .enumerate()
                    .filter(|(x, _)| !current.contains(x))
                    .map(|(_, s)| s.clone())
                    .collect();
            }

            if !current.is_empty() {
                // explanation: c (as in 'consumed') is the longest already matching substring
                // that means we only need to add the "unmatching" part to the frontier
                for i in c..current.len() {
                    let subslice: SmallVec<[usize; 32]> =
                        [&current[0..c], &current[c..i], &current[i + 1..]]
                            .iter()
                            .flat_map(|x| x.iter().copied())
                            .collect();

                    let strrep = subslice.iter().map(|&i| map[&s[i].typ]).collect::<String>();
                    if !seen.contains(&strrep) {
                        seen.insert(strrep);
                        frontier.push_back(subslice);
                    }
                }
            } else {
                return s.into();
            }
        }
        s.into()
    }
}

pub async fn validate_vorgang(vg: &models::Vorgang, srv: &LTZFServer) -> Result<()> {
    let mut stations = vg.stationen.clone();
    stations.sort_by(|s1, s2| s1.zp_start.cmp(&s2.zp_start));
    let parls: BTreeSet<models::Parlament> = stations.iter().map(|s| s.gremium.parlament).collect();
    let s_parls: Vec<_> = parls.iter().map(ToString::to_string).collect();

    let num_classes: i64 = sqlx::query_scalar!(
        "SELECT COUNT(DISTINCT(c_id))
        FROM track_class tc, parlament p
        WHERE 
        p_id = p.id AND
        EXISTS (SELECT 1 FROM UNNEST($1::text[]) as in_parls(ip) WHERE ip = p.value)",
        &s_parls[..]
    )
    .fetch_one(&srv.sqlx_db)
    .await?
    .expect("Expected to find at least one class");
    if num_classes != 1 {
        let reason = format!(
            "The Vorgang contains Stations from more than one legislative class (parliament) in it's body: {:?}, which invalidates it",
            s_parls
        );
        tracing::warn!("{reason}");
        return Err(DataValidationError::MalformedVorgang { reason }.into());
    }

    let mut parls_valid = Vec::new();
    for p in parls {
        tracing::trace!("now in parliament {p}");
        let regex = srv.tracks.tracks.get(&(p, vg.typ)).ok_or(
            DataValidationError::MalformedVorgang { reason: format!("A vorgang of type {} is not specified for parliament {p}. If you believe this to be an error, open an issue: https://codeberg.org/PaZuFa/pazufa-backend/issues", vg.typ) }
            )?;
        tracing::trace!("Tracks retrieved");
        let p_subvec: Vec<models::Station> = stations
            .iter()
            .filter(|x| x.gremium.parlament == p)
            .cloned()
            .collect();
        tracing::trace!("Subvec constructed");
        let p_substr: Vec<char> = p_subvec
            .iter()
            .map(|s| srv.tracks.station_mapping[&s.typ])
            .collect();
        tracing::trace!("Substr constructed: {:?}", p_substr);

        // matching and checking
        let (matched, consumed) = regex.match_regex(&p_substr);
        tracing::trace!("{:?} matched like so: {:?}", p_substr, (matched, consumed));
        if !matched {
            let conflicts: Vec<_> =
                regex.conflicting_stations(&p_subvec, &srv.tracks.station_mapping);
            let conflicts_str: Vec<_> = conflicts
                .iter()
                .map(|s| format!("({} / {})", s.zp_start, s.typ))
                .collect();
            let p_subvec: Vec<_> = p_subvec
                .iter()
                .map(|s| format!("({} / {})", s.zp_start, s.typ))
                .collect();

            let reason = format!(
                "The Vorgang with this station ordering: {:?} has at least one station that is not adhering to the track for parliament {p} and vgtyp {}: {:?}",
                p_subvec, vg.typ, conflicts_str
            );
            tracing::warn!("{reason}");
            parls_valid.push(reason);
        } else if matched && consumed < p_substr.len() {
            let reason = format!(
                "Some stations have been matched and are in valid order, but there are stations left over that cannot be assigned a track: {:?}",
                &p_substr[p_substr.len() - consumed + 1..]
            );
            tracing::warn!("{reason}");
            parls_valid.push(reason);
        }
    }
    if parls_valid.is_empty() {
        Ok(())
    } else {
        let fmt = parls_valid.iter().fold(
            "Track validation Failed. Reasons:\n".to_owned(),
            |a, new| format!("{a}\n{new}"),
        );
        Err(DataValidationError::MalformedVorgang { reason: fmt }.into())
    }
}

#[cfg(test)]
mod test_regex {
    use super::*;
    use crate::utils::testing::{TestSetup, generate};
    use tracing_test::traced_test;

    #[test]
    #[traced_test]
    fn conflicting_large_workload() {
        let tracks = Tracks::new("tracks.toml");
        let rx = &tracks.tracks[&(models::Parlament::By, models::Vorgangstyp::GgLandParl)];
        let starting_datetime = chrono::DateTime::parse_from_rfc3339("2025-12-12T12:12:12+00:00")
            .expect("Is a fixed value")
            .to_utc();
        let mut i = 0;
        let stationen: Vec<_> = "IVAAVVVVVJJJJJVVVVVVVVJJJJJJJJVVVVJJJJG"
            .chars()
            .map(|c| {
                i += 1;
                models::Station {
                    typ: tracks
                        .station_mapping
                        .iter()
                        .find_map(|(k, v)| if *v == c { Some(*k) } else { None })
                        .expect(
                            "Should be findable, because it is a test case and I know what I wrote",
                        ),
                    zp_start: starting_datetime
                        .checked_add_days(chrono::Days::new(i))
                        .expect("adding days should not ever fail"),
                    gremium: models::Gremium {
                        parlament: models::Parlament::By,
                        ..generate::default_gremium()
                    },
                    ..generate::default_station()
                }
            })
            .collect();

        let conflicts = rx.conflicting_stations(&stationen, &tracks.station_mapping);
        assert!(
            !conflicts.is_empty() && conflicts.len() < stationen.len(),
            "{} vs {}",
            conflicts.len(),
            stationen.len()
        );
    }

    #[traced_test]
    #[tokio::test]
    async fn vorgang_validate_starmatching_consumption() {
        let scenario = TestSetup::new("vorgang_validate_memleak_starmatching").await;
        let gremium = models::Gremium {
            parlament: models::Parlament::Bt,
            ..generate::default_gremium()
        };
        let stations = [
            ("2021-11-14T12:57:08Z", models::Stationstyp::ParlVollvlsgn),
            ("2022-01-05T04:56:12Z", models::Stationstyp::ParlGgentwurf),
            ("2022-10-24T13:02:26Z", models::Stationstyp::ParlAusschber),
            ("2023-03-14T08:47:49Z", models::Stationstyp::ParlAblehnung),
            ("2024-06-21T10:58:01Z", models::Stationstyp::ParlAblehnung),
        ];
        let mut i = 0;
        let catastrophe = models::Vorgang {
            stationen: stations
                .iter()
                .map(|(t, typ)| {
                    i += 1;
                    models::Station {
                        zp_start: chrono::DateTime::parse_from_rfc3339(t.to_owned())
                            .unwrap()
                            .into(),
                        gremium: gremium.clone(),
                        typ: *typ,
                        ..generate::random::station(i)
                    }
                })
                .collect(),
            ..generate::random::vorgang(1)
        };
        let res = validate_vorgang(&catastrophe, &scenario.server).await;
        assert!(matches!(
            res,
            Err(crate::error::LTZFError::Validation { .. })
        ));
        scenario.teardown().await;
    }
    #[test]
    fn conflicting_stations_full_fail() {
        let rx = "[RE]*S?I((VA*(Z|VJGK|VN|VA*(Z|VJGK|VN)))|Z)";
        let rx = Regex::new(rx);
        let v = models::Vorgang {
            stationen: vec![models::Station {
                typ: models::Stationstyp::ParlAusschber,
                ..generate::default_station()
            }],
            ..generate::default_vorgang()
        };
        let tracks = Tracks::new("tracks.toml");

        let csresult = rx.conflicting_stations(&v.stationen, &tracks.station_mapping);
        assert_eq!(csresult.len(), 1, "{:?}", csresult);
        assert_eq!(csresult[0], v.stationen[0]);
    }

    #[tokio::test]
    async fn validate_vorgang_full_fail() {
        let scenario = TestSetup::new("validate_vorgang_fine").await;
        let vorgang = models::Vorgang {
            stationen: vec![models::Station {
                typ: models::Stationstyp::ParlAusschber,
                ..generate::default_station()
            }],
            ..generate::default_vorgang()
        };
        let result = validate_vorgang(&vorgang, &scenario.server).await;
        assert!(result.is_err(), "{:?}", result);
        scenario.teardown().await;
    }

    #[tokio::test]
    async fn validate_vorgang_formalia_wrong() {
        let scenario = TestSetup::new("validate_vorgang_formalia").await;
        let vorgang = generate::default_vorgang();
        let vorgang = models::Vorgang {
            typ: models::Vorgangstyp::GgLandVolk,
            ..vorgang
        };
        let result = validate_vorgang(&vorgang, &scenario.server).await;
        assert!(
            match result {
                Err(crate::error::LTZFError::Validation { ref source }) =>
                    matches!(**source, DataValidationError::MalformedVorgang { .. }),
                _ => false,
            },
            "{:?}",
            result
        );
        scenario.teardown().await;
    }

    #[test]
    #[traced_test]
    fn match_rx_endless_loop() {
        let rx = Regex::new("[RE]*S?I((VA*(Z|VJGK|VN|VA*(Z|VJGK|VN)))|Z)");
        let to_match = ['R'];
        assert!(matches!(rx.match_regex(&to_match), (true, x) if x == 1));
    }
    #[test]
    fn match_rx_fail() {
        let trackmap = Tracks::new("tracks.toml");
        let rx = "[RE]*S?I((VA*(Z|VJGK|VN|VA*(Z|VJGK|VN)))|Z)";
        let rx = Regex::new(rx);
        let v = models::Vorgang {
            stationen: vec![models::Station {
                typ: models::Stationstyp::ParlAusschber,
                ..generate::default_station()
            }],
            ..generate::default_vorgang()
        };
        let strong: Vec<_> = v
            .stationen
            .iter()
            .map(|s| trackmap.station_mapping[&s.typ])
            .collect();
        let slen = v.stationen.len();
        let (b, c) = rx.match_regex(&strong);
        assert!(
            matches!((b, c), (false, 0)),
            "{slen} / {:?}\n || \n ({b};{c})",
            strong
        );
    }

    fn strrxmatch(rx: &str, st: &str) -> (bool, usize) {
        let vrx: Vec<_> = rx.chars().collect();
        let vst: Vec<_> = st.chars().collect();
        Regex::match_regex(&Regex::parse_regex(&vrx), &vst)
    }
    #[test]
    fn plain_chars() {
        let input = "ABC";
        let input: Vec<char> = input.chars().collect();
        let output = Regex::Conc(vec![Regex::Lit('A'), Regex::Lit('B'), Regex::Lit('C')]);
        assert_eq!(output, Regex::parse_regex(&input));
    }

    #[test]
    fn star_on_char() {
        let input = "BA*";
        let input: Vec<char> = input.chars().collect();
        let output = Regex::Conc(vec![
            Regex::Lit('B'),
            Regex::Star(Box::new(Regex::Lit('A'))),
        ]);
        assert_eq!(output, Regex::parse_regex(&input));
    }

    #[test]
    fn choice_concat() {
        let input = "BB|AA";
        let input: Vec<char> = input.chars().collect();
        let output = Regex::Choice(
            Regex::Conc(vec![Regex::Lit('B'), Regex::Lit('B')]).into(),
            Regex::Conc(vec![Regex::Lit('A'), Regex::Lit('A')]).into(),
        );
        assert_eq!(output, Regex::parse_regex(&input));
    }

    #[test]
    fn choice() {
        let input = "B|A";
        let input: Vec<char> = input.chars().collect();
        let output = Regex::Choice(Box::new(Regex::Lit('B')), Box::new(Regex::Lit('A')));
        assert_eq!(output, Regex::parse_regex(&input));
    }

    #[test]
    fn choice_braces() {
        let input = "B|(AB)";
        let input: Vec<char> = input.chars().collect();
        let output = Regex::Choice(
            Box::new(Regex::Lit('B')),
            Box::new(Regex::Conc(vec![Regex::Lit('A'), Regex::Lit('B')])),
        );
        assert_eq!(output, Regex::parse_regex(&input));
    }

    #[test]
    fn brace_then_star() {
        let input = "(AB)*";
        let input: Vec<char> = input.chars().collect();
        let output = Regex::Star(Regex::Conc(vec![Regex::Lit('A'), Regex::Lit('B')]).into());
        assert_eq!(output, Regex::parse_regex(&input));
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
                Regex::Choice(Regex::Lit('B').into(), Regex::Lit('A').into()).into(),
                Regex::Lit('C').into(),
            )
            .into(),
        );
        assert_eq!(output, Regex::parse_regex(&input));
    }

    #[test]
    fn questionmark() {
        let input = "A?";
        let input: Vec<char> = input.chars().collect();
        let output = Regex::Choice(Regex::Lit('A').into(), Regex::Empty.into());
        assert_eq!(output, Regex::parse_regex(&input));
    }
    #[test]
    fn plus() {
        let input = "A+";
        let input: Vec<char> = input.chars().collect();
        let output = Regex::Conc(vec![Regex::Lit('A'), Regex::Star(Regex::Lit('A').into())]);
        assert_eq!(output, Regex::parse_regex(&input));
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
