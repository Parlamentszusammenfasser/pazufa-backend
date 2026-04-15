use crate::{LTZFServer, Result, error::DataValidationError};
use openapi::models;
use serde::Deserialize;
use smallvec::SmallVec;
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, HashSet, VecDeque},
    mem,
};

#[derive(Deserialize)]
struct RawTracks {
    version: String,
    stations: BTreeMap<char, models::Stationstyp>,
    tracks: BTreeMap<models::Parlament, BTreeMap<models::Vorgangstyp, String>>,
}

pub struct Tracks {
    pub version: String,
    pub station_mapping: BTreeMap<models::Stationstyp, char>,
    pub trackmap: BTreeMap<(models::Parlament, models::Vorgangstyp), Regex>,
}
impl Tracks {
    pub fn new(path: &str) -> Self {
        let file = std::fs::read_to_string(std::path::Path::new(path)).unwrap_or_else(|error| {
            panic!("Expected to find a tracks file configured as: `{path}`. Error: {error}")
        });
        let rt: RawTracks =
            toml::from_str(&file).unwrap_or_else(|e| panic!("Trackfile could not be loaded: {e}"));
        let trackmap: BTreeMap<_, Regex> = rt
            .tracks
            .iter()
            .flat_map(|(parl, v)| {
                v.iter().map(|(&vtyp, rx)| {
                    ((*parl, vtyp), {
                        tracing::trace!("parsing {rx}");
                        Regex::new(rx).expect("Malformed tracks file")
                    })
                })
            })
            .collect();
        let sm: BTreeMap<_, char> = rt.stations.iter().map(|(k, v)| (*v, *k)).collect();
        Self {
            version: rt.version,
            station_mapping: sm,
            trackmap,
        }
    }
}

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

impl Regex {
    pub fn new(re: &str) -> Result<Regex> {
        let x: Vec<_> = re.chars().collect();
        Self::parse_regex(&x)
    }

    // takes `[ABCDEF]` returns Regex("A|B|C|D|E|F")
    #[allow(
        clippy::arithmetic_side_effects,
        reason = "only overflows for re.len() == usize::MAX, which is asserted out before"
    )]
    fn parse_choicebox(re: &[char]) -> (Regex, usize) {
        let mut last_regex = Regex::Empty;
        let mut ci = 0;
        assert!(
            re.len() < usize::MAX,
            "If this happens your memory will have overflowed anyways"
        );
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

    // LIMITATION: parses a*aa into a*aa, and does no reordering.
    // This assumes that stars are always at the end of expressions of the same kind:
    // a*aa <=> aaa*

    #[allow(
        clippy::arithmetic_side_effects,
        reason = "only overflows for re.len() == usize::MAX, which is asserted out before"
    )]
    fn parse_regex(re: &[char]) -> Result<Regex> {
        if re.is_empty() {
            return Ok(Regex::Empty);
        }
        assert!(
            re.len() < usize::MAX,
            "If this happens your memory will have overflowed anyways"
        );

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
                        return Err(DataValidationError::RegexError {
                            regex: re.into(),
                            ci,
                            c,
                            parsed_so_far: rx,
                        }
                        .into());
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
                        return Err(DataValidationError::RegexError {
                            regex: re.into(),
                            ci,
                            c,
                            parsed_so_far: rx,
                        }
                        .into());
                    }
                }
                ('+', Regex::Conc(child)) => {
                    if let Some(last_child) = child.last() {
                        child.push(Regex::Star(Box::new(last_child.clone())));
                    } else {
                        return Err(DataValidationError::RegexError {
                            regex: re.into(),
                            ci,
                            c,
                            parsed_so_far: rx,
                        }
                        .into());
                    }
                }
                _ => {
                    return Err(DataValidationError::RegexError {
                        regex: re.into(),
                        ci,
                        c,
                        parsed_so_far: rx,
                    }
                    .into());
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

    // takes `(abc(de)fg(h))` and returns `abc(de)fg(h)`

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

        while let Some(current) = frontier.pop_front() {
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

            if current.is_empty() {
                return s.into();
            }
            // explanation: c (as in 'consumed') is the longest already matching substring
            // that means we only need to add the "unmatching" part to the frontier
            for i in c..current.len() {
                let mut subslice: SmallVec<[usize; 32]> = current.clone();
                _ = subslice.remove(i);

                let strrep = subslice.iter().map(|&i| map[&s[i].typ]).collect::<String>();
                if !seen.contains(&strrep) {
                    seen.insert(strrep);
                    frontier.push_back(subslice);
                }
            }
        }
        s.into()
    }

    fn simplify(self) -> Self {
        match self {
            Regex::Star(inner) => Regex::Star(inner.simplify().into()),
            Regex::Choice(left, right) => factor_prefix(canonicalize_choice(Regex::Choice(
                left.simplify().into(),
                right.simplify().into(),
            ))),
            Regex::Conc(inner) => Regex::Conc(inner.into_iter().map(Self::simplify).collect()),
            other => other,
        }
        .reduce()
    }

    fn reduce(self) -> Self {
        match self {
            Regex::Star(inner) if *inner == Regex::Empty => Regex::Empty,
            Regex::Star(inner) if matches!(*inner, Regex::Star(_)) => *inner,

            Regex::Choice(left, right) if left == right => *left,
            Regex::Choice(left, right)
                if *left == Regex::Empty && matches!(*right, Regex::Star(_)) =>
            {
                *right
            }
            Regex::Choice(left, right)
                if *right == Regex::Empty && matches!(*left, Regex::Star(_)) =>
            {
                *left
            }
            Regex::Choice(left, right)
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
            Regex::Choice(left, right)
                if matches!(*left, Regex::Star(_)) && {
                    if let Regex::Star(inner) = &*left {
                        **inner == *right
                    } else {
                        false
                    }
                } =>
            {
                *left
            }

            Regex::Conc(inner) => reduce_conc(inner),
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
            Regex::Empty
        } else {
            Regex::Conc(parts)
        }
    }
}

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

fn reduce_conc(parts: Vec<Regex>) -> Regex {
    let mut flat = VecDeque::new();
    for part in parts {
        if let Regex::Conc(inner) = part {
            flat.extend(inner.into_iter());
        } else {
            flat.push_back(part);
        }
    }

    flat.retain(|r| *r != Regex::Empty);

    let mut absorbed = Vec::with_capacity(flat.len());
    while let Some(head) = flat.pop_front() {
        let head = if let Some(next) = flat.get_mut(0) {
            match (head, next) {
                (mut current, next)
                    if matches!(current, Regex::Star(_)) && {
                        if let Regex::Star(inner) = &current {
                            **inner == *next
                        } else {
                            false
                        }
                    } =>
                {
                    mem::swap(&mut current, next);
                    current
                }
                (Regex::Star(head), Regex::Star(next)) if *head == **next => {
                    continue;
                }
                (head, _) => head,
            }
        } else {
            head
        };
        absorbed.push(head);
    }

    Regex::from_parts(absorbed)
}

fn canonicalize_choice(choice: Regex) -> Regex {
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

    let mut flattened = flatten_choice(choice);
    flattened.sort();
    flattened.dedup();

    flattened
        .into_iter()
        .rev()
        .reduce(|acc, item| Regex::Choice(item.into(), acc.into()))
        .unwrap_or(Regex::Empty)
}

fn factor_prefix(choice: Regex) -> Regex {
    if let Regex::Choice(left, right) = choice {
        let left: &mut dyn Iterator<Item = Regex> = if let Regex::Conc(left) = *left {
            &mut left.into_iter()
        } else {
            &mut std::iter::once(*left)
        };
        let right: &mut dyn Iterator<Item = Regex> = if let Regex::Conc(right) = *right {
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
                if left == right {
                    prefix.push(left);
                } else {
                    new_left.push(left);
                    new_right.push(right);
                    break;
                }
            } else {
                new_left.push(left);
                break;
            }
        }
        new_left.extend(left);
        new_right.extend(right);

        let new_choice = Regex::Choice(
            Regex::from_parts(new_left).into(),
            Regex::from_parts(new_right).into(),
        );

        if prefix.is_empty() {
            new_choice
        } else {
            prefix.push(new_choice);
            Regex::from_parts(prefix)
        }
    } else {
        choice
    }
}

pub async fn validate_vorgang(vg: &models::Vorgang, srv: &LTZFServer) -> Result<()> {
    let mut stations = vg.stationen.clone();
    stations.sort_by(|s1, s2| s1.zp_start.cmp(&s2.zp_start));
    let parls: BTreeSet<models::Parlament> = stations.iter().map(|s| s.gremium.parlament).collect();
    let s_parls: Vec<_> = parls.iter().map(ToString::to_string).collect();

    let num_classes: i64 = sqlx::query_scalar!(
        r#"SELECT COUNT(DISTINCT(c_id)) as "cnt!"
        FROM track_class tc, parlament p
        WHERE 
        p_id = p.id AND
        EXISTS (SELECT 1 FROM UNNEST($1::text[]) as in_parls(ip) WHERE ip = p.value)"#,
        &s_parls[..]
    )
    .fetch_one(&srv.sqlx_db)
    .await?;
    if num_classes != 1 {
        let reason = format!(
            "The Vorgang contains Stations from more than one legislative class (parliament) in it's body: {s_parls:?}, which invalidates it"
        );
        tracing::warn!("{reason}");
        return Err(DataValidationError::MalformedVorgang { reason }.into());
    }
    if stations
        .iter()
        .any(|s| !srv.tracks.station_mapping.contains_key(&s.typ))
    {
        return Err(crate::error::DataValidationError::MalformedVorgang {
            reason: "Vorgang contains unmapped station type (probably 'sonstig')".to_owned(),
        }
        .into());
    }
    let mut parls_valid = Vec::new();
    for p in parls {
        tracing::trace!("now in parliament {p}");
        let regex = srv.tracks.trackmap.get(&(p, vg.typ)).ok_or(
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
                "Some stations have been matched and are in valid order, but there are stations left over that cannot be assigned to a track: {:?}",
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
#[allow(clippy::expect_used, reason = "panics in test code are fine")]
#[allow(
    clippy::arithmetic_side_effects,
    reason = "is either checked in test or can panic"
)]
mod test_regex {
    use super::*;
    use crate::utils::testing::{TestSetup, generate};
    use rand::{Rng, rng, seq::SliceRandom};
    use tracing_test::traced_test;

    fn station_mapping() -> BTreeMap<models::Stationstyp, char> {
        let mut map = BTreeMap::new();
        map.insert(models::Stationstyp::PreparlRegent, 'R');
        map.insert(models::Stationstyp::PreparlEckpup, 'E');
        map.insert(models::Stationstyp::PreparlRegbsl, 'S');
        map.insert(models::Stationstyp::ParlInitiativ, 'I');
        map.insert(models::Stationstyp::ParlVollvlsgn, 'V');
        map.insert(models::Stationstyp::ParlAkzeptanz, 'J');
        map.insert(models::Stationstyp::ParlZurueckgz, 'Z');
        map.insert(models::Stationstyp::ParlAblehnung, 'N');
        map.insert(models::Stationstyp::ParlAusschber, 'A');
        map.insert(models::Stationstyp::PostparlKraft, 'K');
        map.insert(models::Stationstyp::PostparlGsblt, 'G');
        map.insert(models::Stationstyp::PreparlVbegde, 'B');
        map.insert(models::Stationstyp::ParlGgentwurf, 'W');
        map.insert(models::Stationstyp::PostparlVesja, 'Y');
        map.insert(models::Stationstyp::PostparlVesne, 'X');
        map
    }
    #[tokio::test]
    async fn sonstig_station_run() {
        let scenario = TestSetup::new("sonstig_station_run").await;

        let vg = models::Vorgang {
            stationen: vec![models::Station {
                typ: models::Stationstyp::Sonstig,
                ..generate::default_station()
            }],
            ..generate::default_vorgang()
        };
        let res = validate_vorgang(&vg, &scenario.server).await;
        assert!(matches!(
            res,
            Err(crate::error::LTZFError::Validation { .. })
        ));
        scenario.teardown().await;
    }

    #[test]
    #[traced_test]
    fn conflicting_large_workload() {
        let rx =
            Regex::new("((E*R+)?S)?I((VA*(Z|VJGK|VN|VA*(Z|VJGK|VN)))|Z)").expect("const string");
        let station_mapping = station_mapping();
        let starting_datetime = chrono::DateTime::parse_from_rfc3339("2025-12-12T12:12:12+00:00")
            .expect("Is a fixed value")
            .to_utc();
        let mut i = 0;
        let stationen: Vec<_> = "IVAAVVVVVJJJJJVVVVVVVVJJJJJJJJVVVVJJJJG"
            .chars()
            .map(|c| {
                i += 1;
                models::Station {
                    typ: station_mapping
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

        let conflicts = rx.conflicting_stations(&stationen, &station_mapping);
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
                            .expect("Hardcoded Date parsing failed")
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
        let rx = Regex::new(rx).expect("const string");
        let v = models::Vorgang {
            stationen: vec![models::Station {
                typ: models::Stationstyp::ParlAusschber,
                ..generate::default_station()
            }],
            ..generate::default_vorgang()
        };

        let csresult = rx.conflicting_stations(&v.stationen, &station_mapping());
        assert_eq!(csresult.len(), 1, "{csresult:?}");
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
        assert!(result.is_err(), "{result:?}");
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
            "{result:?}"
        );
        scenario.teardown().await;
    }

    #[test]
    #[traced_test]
    fn match_rx_endless_loop() {
        let rx = Regex::new("[RE]*S?I((VA*(Z|VJGK|VN|VA*(Z|VJGK|VN)))|Z)").expect("const string");
        let to_match = ['R'];
        assert!(matches!(rx.match_regex(&to_match), (true, x) if x == 1));
    }
    #[test]
    fn match_rx_fail() {
        let rx = "[RE]*S?I((VA*(Z|VJGK|VN|VA*(Z|VJGK|VN)))|Z)";
        let rx = Regex::new(rx).expect("const string");
        let v = models::Vorgang {
            stationen: vec![models::Station {
                typ: models::Stationstyp::ParlAusschber,
                ..generate::default_station()
            }],
            ..generate::default_vorgang()
        };
        let station_mapping = station_mapping();
        let strong: Vec<_> = v
            .stationen
            .iter()
            .map(|s| station_mapping[&s.typ])
            .collect();
        let slen = v.stationen.len();
        let (b, c) = rx.match_regex(&strong);
        assert!(
            matches!((b, c), (false, 0)),
            "{slen} / {strong:?}\n || \n ({b};{c})"
        );
    }

    fn strrxmatch(rx: &str, st: &str) -> (bool, usize) {
        let vrx: Vec<_> = rx.chars().collect();
        let vst: Vec<_> = st.chars().collect();
        Regex::match_regex(
            &Regex::parse_regex(&vrx).expect("test case setup failed"),
            &vst,
        )
    }
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

    #[test]
    fn canonicalizes_choicebox() {
        let mut r = rng();
        let mut options = ['A', 'B', 'C', 'D', 'E', 'F'];
        options.shuffle(&mut r);

        let randomized_choice = match canonicalize_choice(options.iter().fold(
            Regex::Empty,
            |acc, e| {
                if r.random_bool(0.5) {
                    Regex::Choice(acc.into(), Regex::Lit(*e).into())
                } else {
                    Regex::Choice(Regex::Lit(*e).into(), acc.into())
                }
            },
        )) {
            Regex::Choice(left, right) if *left == Regex::Empty => *right,
            e => panic!(
                "Canonization did not return Choice(Empty, canonized) as it was expected. Actual result: {e:?}"
            ),
        };

        let parsed_choice =
            Regex::parse_regex(&"[ABCDEF]".chars().collect::<Vec<_>>()).expect("const input");

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

        let first_shuffle = match canonicalize_choice(options.iter().fold(
            Regex::Empty,
            |acc, e| {
                if r.random_bool(0.5) {
                    Regex::Choice(acc.into(), Regex::Lit(*e).into())
                } else {
                    Regex::Choice(Regex::Lit(*e).into(), acc.into())
                }
            },
        )) {
            Regex::Choice(left, right) if *left == Regex::Empty => *right,
            e => panic!(
                "Canonization did not return Choice(Empty, canonized) as it was expected. Actual result: {e:?}"
            ),
        };

        options.shuffle(&mut r);

        let second_shuffle = match canonicalize_choice(options.iter().fold(
            Regex::Empty,
            |acc, e| {
                if r.random_bool(0.5) {
                    Regex::Choice(acc.into(), Regex::Lit(*e).into())
                } else {
                    Regex::Choice(Regex::Lit(*e).into(), acc.into())
                }
            },
        )) {
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
            canonicalize_choice(Regex::Choice(Regex::Empty.into(), Regex::Empty.into())),
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
        let choice = match canonicalize_choice(options.iter().fold(Regex::Empty, |acc, e| {
            if r.random_bool(0.5) {
                Regex::Choice(acc.into(), Regex::Lit(*e).into())
            } else {
                Regex::Choice(Regex::Lit(*e).into(), acc.into())
            }
        })) {
            Regex::Choice(left, right) if *left == Regex::Empty => *right,
            e => panic!(
                "Canonization did not return Choice(Empty, canonized) as it was expected. Actual result: {e:?}"
            ),
        };

        assert_eq!(
            choice,
            canonicalize_choice(Regex::Choice(
                Regex::Lit(chr).into(),
                Regex::Lit(o_chr).into()
            ))
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

        let reduced = reduce_conc(parts);

        let expected = Regex::Conc(
            std::iter::repeat_n(Regex::Lit(chr), min)
                .chain(std::iter::once(Regex::Star(Regex::Lit(chr).into())))
                .collect::<Vec<_>>(),
        );

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
            factor_prefix(Regex::Choice(re.clone().into(), re.clone().into(),)),
            expected
        );
    }

    #[test]
    fn prefix_factoring_of_subset() {
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
        let right_long = Regex::Choice(
            Regex::from_parts(prefix.clone()).into(),
            Regex::from_parts(longer).into(),
        );

        prefix.push(Regex::Choice(
            Regex::Empty.into(),
            Regex::from_parts(suffix).into(),
        ));
        let expected = Regex::from_parts(prefix);

        assert_eq!(
            factor_prefix(left_long).simplify(),
            expected,
            "Failed to correctly factor prefix if right is prefix of left. Other way not tested."
        );
        assert_eq!(
            factor_prefix(right_long).simplify(),
            expected,
            "Failed to correctly factor prefix if left is prefix of right. Other way succeeded."
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
            factor_prefix(choice),
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
            factor_prefix(choice.clone()),
            choice,
            "Failed to factor without common prefix."
        );
    }
}
