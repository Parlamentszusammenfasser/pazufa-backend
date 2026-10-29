//! Field-level input constraints, shared by deserialization and `arbitrary` generation.
//!
//! Every restriction is written down exactly once, as [`Rule::validate`]. Two consumers use it:
//! - serde, via `#[serde(deserialize_with = "Rule::de")]` (and `de_opt`, `de_vec`), which rejects
//!   input that fails validation.
//! - arbitrary, via `#[arbitrary(with = Rule::arb)]` (and `arb_opt`, `arb_vec`), which builds a
//!   value with [`Rule::generate`] and then runs that same `validate` on it. A generator that
//!   drifts from its rule therefore fails loudly instead of producing input the API would reject.
//!
//! Pair the attributes on each field:
//! ```ignore
//! #[serde(deserialize_with = "Text::de_opt", default)]
//! #[arbitrary(with = Text::arb_opt)]
//! pub kurztitel: Option<String>,
//! ```
//!
//! Vectors are treated as sets: `de_vec` rejects duplicate elements and `arb_vec` never produces
//! them. Vectors of structured types that carry no field rule use [`Set`] for the same guarantee.

use arbitrary::{Arbitrary, Unstructured};
use serde::{Deserialize, Deserializer, de::DeserializeOwned};
use std::marker::PhantomData;
use std::str::FromStr;

pub(crate) trait Rule {
    type Value: DeserializeOwned;

    /// The single source of truth for this constraint.
    fn validate(v: &Self::Value) -> Result<(), &'static str>;

    /// Produce a value for `arbitrary`. It must satisfy `validate`; this is checked in [`arb`].
    fn generate(u: &mut Unstructured<'_>) -> arbitrary::Result<Self::Value>;
}

fn de<'de, R: Rule, D: Deserializer<'de>>(d: D) -> Result<R::Value, D::Error> {
    let v = R::Value::deserialize(d)?;
    R::validate(&v).map_err(serde::de::Error::custom)?;
    Ok(v)
}

fn de_opt<'de, R: Rule, D: Deserializer<'de>>(d: D) -> Result<Option<R::Value>, D::Error> {
    let v = Option::<R::Value>::deserialize(d)?;
    if let Some(v) = &v {
        R::validate(v).map_err(serde::de::Error::custom)?;
    }
    Ok(v)
}

fn de_vec<'de, R: Rule, D: Deserializer<'de>>(d: D) -> Result<Vec<R::Value>, D::Error>
where
    R::Value: PartialEq,
{
    let v = Vec::<R::Value>::deserialize(d)?;
    for x in &v {
        R::validate(x).map_err(serde::de::Error::custom)?;
    }
    check_unique(&v).map_err(serde::de::Error::custom)?;
    Ok(v)
}

fn arb<R: Rule>(u: &mut Unstructured<'_>) -> arbitrary::Result<R::Value> {
    let v = R::generate(u)?;
    R::validate(&v).map_err(|_| arbitrary::Error::IncorrectFormat)?;
    Ok(v)
}

/// Adapter so `Option`/`Vec` generation reuses arbitrary's own length/presence distribution.
struct Generated<R: Rule>(R::Value, PhantomData<R>);

impl<'a, R: Rule> Arbitrary<'a> for Generated<R> {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self(arb::<R>(u)?, PhantomData))
    }
}

fn arb_opt<R: Rule>(u: &mut Unstructured<'_>) -> arbitrary::Result<Option<R::Value>> {
    Ok(Option::<Generated<R>>::arbitrary(u)?.map(|g| g.0))
}

fn arb_vec<R: Rule>(u: &mut Unstructured<'_>) -> arbitrary::Result<Vec<R::Value>>
where
    R::Value: PartialEq,
{
    let v = Vec::<Generated<R>>::arbitrary(u)?
        .into_iter()
        .map(|g| g.0)
        .collect();
    Ok(dedup(v))
}

/// Error if any two elements of `v` are the same according to `same`.
/// Quadratic, but these lists are short and most element types are neither `Hash` nor `Ord`.
fn check_unique_by<T>(v: &[T], same: impl Fn(&T, &T) -> bool) -> Result<(), String> {
    for (i, a) in v.iter().enumerate() {
        if let Some(j) = v.iter().skip(i.saturating_add(1)).position(|b| same(a, b)) {
            return Err(format!(
                "This list must not contain duplicates, but entries {i} and {} are equal",
                i.saturating_add(1).saturating_add(j)
            ));
        }
    }
    Ok(())
}

fn check_unique<T: PartialEq>(v: &[T]) -> Result<(), String> {
    check_unique_by(v, T::eq)
}

/// Keep the first of all elements that are the same according to `same`, preserving order.
fn dedup_by<T>(v: Vec<T>, same: impl Fn(&T, &T) -> bool) -> Vec<T> {
    let mut out: Vec<T> = Vec::with_capacity(v.len());
    for x in v {
        if !out.iter().any(|o| same(o, &x)) {
            out.push(x);
        }
    }
    out
}

fn dedup<T: PartialEq>(v: Vec<T>) -> Vec<T> {
    dedup_by(v, T::eq)
}

/// A vector of pairwise distinct elements, for element types without a field [`Rule`]
/// (their own constraints are enforced by their `Deserialize`/`Arbitrary` impls).
/// ```ignore
/// #[serde(deserialize_with = "Set::de")]
/// #[arbitrary(with = Set::arb)]
/// pub initiatoren: Vec<models::Autor>,
/// ```
pub(crate) struct Set;

impl Set {
    pub(crate) fn de<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de> + PartialEq,
    {
        let v = Vec::<T>::deserialize(d)?;
        check_unique(&v).map_err(serde::de::Error::custom)?;
        Ok(v)
    }

    pub(crate) fn arb<'a, T>(u: &mut Unstructured<'a>) -> arbitrary::Result<Vec<T>>
    where
        T: Arbitrary<'a> + PartialEq,
    {
        Ok(dedup(Vec::<T>::arbitrary(u)?))
    }
}

/// Element types whose identity (as the database sees it) is only part of their data,
/// e.g. at most one lobbyregister entry per organisation. Used by [`KeyedSet`].
pub(crate) trait SetKey {
    type Key: PartialEq;
    fn key(&self) -> &Self::Key;
}

fn same_key<T: SetKey>(a: &T, b: &T) -> bool {
    a.key() == b.key()
}

/// Like [`Set`], but elements only have to differ in their [`SetKey::key`].
/// ```ignore
/// #[serde(deserialize_with = "KeyedSet::de")]
/// #[arbitrary(with = KeyedSet::arb)]
/// pub lobbyregister: Vec<models::Lobbyregistereintrag>,
/// ```
pub(crate) struct KeyedSet;

impl KeyedSet {
    pub(crate) fn validate<T: SetKey>(v: &[T]) -> Result<(), String> {
        check_unique_by(v, same_key)
    }

    pub(crate) fn de<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de> + SetKey,
    {
        let v = Vec::<T>::deserialize(d)?;
        Self::validate(&v).map_err(serde::de::Error::custom)?;
        Ok(v)
    }

    pub(crate) fn arb<'a, T>(u: &mut Unstructured<'a>) -> arbitrary::Result<Vec<T>>
    where
        T: Arbitrary<'a> + SetKey,
    {
        Ok(dedup_by(Vec::<T>::arbitrary(u)?, same_key))
    }
}

/// Exposes the generic functions as inherent associated functions (`Text::de_opt`, ...),
/// so attribute paths stay short and need no trait import at the use site.
macro_rules! rule_api {
    ($($rule:ident),* $(,)?) => {$(
        #[allow(dead_code, reason = "not every rule is used in every shape")]
        impl $rule {
            pub(crate) fn validate(v: &<Self as Rule>::Value) -> Result<(), &'static str> {
                <Self as Rule>::validate(v)
            }
            pub(crate) fn de<'de, D: Deserializer<'de>>(d: D) -> Result<<Self as Rule>::Value, D::Error> {
                de::<Self, D>(d)
            }
            pub(crate) fn de_opt<'de, D: Deserializer<'de>>(d: D) -> Result<Option<<Self as Rule>::Value>, D::Error> {
                de_opt::<Self, D>(d)
            }
            pub(crate) fn de_vec<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<<Self as Rule>::Value>, D::Error>
            where <Self as Rule>::Value: PartialEq {
                de_vec::<Self, D>(d)
            }
            pub(crate) fn arb(u: &mut Unstructured<'_>) -> arbitrary::Result<<Self as Rule>::Value> {
                arb::<Self>(u)
            }
            pub(crate) fn arb_opt(u: &mut Unstructured<'_>) -> arbitrary::Result<Option<<Self as Rule>::Value>> {
                arb_opt::<Self>(u)
            }
            pub(crate) fn arb_vec(u: &mut Unstructured<'_>) -> arbitrary::Result<Vec<<Self as Rule>::Value>>
            where <Self as Rule>::Value: PartialEq {
                arb_vec::<Self>(u)
            }
        }
    )*};
}

rule_api!(Text, FullText, Link, HexDigest, Timestamp);

// ---------------------------------------------------------------------------------------------
// rules
// ---------------------------------------------------------------------------------------------

/// Non-empty, no `\0`, no html.
pub(crate) struct Text;

/// Non-empty, no `\0`. Html is tolerated (full texts of documents).
pub(crate) struct FullText;

/// A [`Text`] that is an absolute link (has scheme and authority).
pub(crate) struct Link;

/// Hash digest: 40 to 64 hexadecimal characters.
pub(crate) struct HexDigest;

/// Point in time within [0000-01-01, 9999-01-01).
pub(crate) struct Timestamp;

const ERR_EMPTY: &str = "Strings must not be empty. please fill them with TODO markers, with a sensible default or with processed content";

/// Fallback for generated strings that end up empty, e.g. when the random input runs out.
const GENERATED_FALLBACK: &str = "BLANK";

/// Arbitrary non-empty string without `\0` and `<`.
/// `ammonia::is_html` only flags non-character tokens (tags, comments, doctypes, NUL),
/// all of which need one of those two characters.
fn plain_string(u: &mut Unstructured<'_>) -> arbitrary::Result<String> {
    let s: String = String::arbitrary(u)?
        .chars()
        .filter(|c| !matches!(c, '\0' | '<'))
        .collect();
    Ok(if s.is_empty() {
        GENERATED_FALLBACK.to_owned()
    } else {
        s
    })
}

fn choose_chars(
    u: &mut Unstructured<'_>,
    alphabet: &[u8],
    len: usize,
) -> arbitrary::Result<String> {
    (0..len)
        .map(|_| u.choose(alphabet).map(|b| char::from(*b)))
        .collect()
}

impl Rule for Text {
    type Value = String;
    fn validate(s: &String) -> Result<(), &'static str> {
        if s.is_empty() {
            Err(ERR_EMPTY)
        } else if s.contains('\0') {
            Err("This text contains '\\0', which is rejected due to being the C-String terminator")
        } else if ammonia::is_html(s) {
            Err(
                "String was recognized to contain html structure. This might indicate cross-site scripting",
            )
        } else {
            Ok(())
        }
    }
    fn generate(u: &mut Unstructured<'_>) -> arbitrary::Result<String> {
        plain_string(u)
    }
}

impl Rule for FullText {
    type Value = String;
    fn validate(s: &String) -> Result<(), &'static str> {
        if s.is_empty() {
            Err(ERR_EMPTY)
        } else if s.contains('\0') {
            Err("This text contains '\\0'. Please no shenanigans, law texts are boring")
        } else {
            Ok(())
        }
    }
    fn generate(u: &mut Unstructured<'_>) -> arbitrary::Result<String> {
        plain_string(u)
    }
}

impl Rule for Link {
    type Value = String;
    fn validate(s: &String) -> Result<(), &'static str> {
        Text::validate(s)?;
        let uri =
            axum::http::Uri::from_str(s).map_err(|_| "This vector should only contain links")?;
        if uri.scheme().is_none() || uri.authority().is_none() {
            return Err("This vector should only contain links with a scheme (like https)");
        }
        Ok(())
    }
    fn generate(u: &mut Unstructured<'_>) -> arbitrary::Result<String> {
        const HOST: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
        let host_len = u.int_in_range(1..=16)?;
        let host = choose_chars(u, HOST, host_len)?;
        let path: u32 = u.arbitrary()?;
        Ok(format!("https://{host}.example/{path:x}"))
    }
}

impl Rule for HexDigest {
    type Value = String;
    fn validate(s: &String) -> Result<(), &'static str> {
        if !(40usize..=64).contains(&s.len()) {
            Err("Hash value must be between 40 and 64 characters long")
        } else if !s.chars().all(|c| c.is_ascii_hexdigit()) {
            Err("String contains non-hexadecimal digit characters")
        } else {
            Ok(())
        }
    }
    fn generate(u: &mut Unstructured<'_>) -> arbitrary::Result<String> {
        let len = u.int_in_range(40..=64)?;
        choose_chars(u, b"0123456789abcdef", len)
    }
}

/// 0000-01-01T00:00:00Z
const TS_MIN: i64 = -62_167_219_200;
/// 9999-01-01T00:00:00Z (exclusive)
const TS_END: i64 = 253_370_764_800;

impl Rule for Timestamp {
    type Value = chrono::DateTime<chrono::Utc>;
    fn validate(dt: &Self::Value) -> Result<(), &'static str> {
        let secs = dt.timestamp();
        if secs < TS_MIN {
            Err("DateTime is too early")
        } else if secs >= TS_END {
            Err("DateTime is too late")
        } else {
            Ok(())
        }
    }
    fn generate(u: &mut Unstructured<'_>) -> arbitrary::Result<Self::Value> {
        chrono::DateTime::from_timestamp(
            u.int_in_range(TS_MIN..=TS_END.saturating_sub(1))?,
            u.int_in_range(0..=999_999_999)?,
        )
        .ok_or(arbitrary::Error::IncorrectFormat)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn samples<R: Rule>(n: u64) -> Vec<R::Value> {
        use rand::{Rng, SeedableRng, rngs::StdRng};
        (0..n)
            .map(|seed| {
                let mut data = vec![0u8; 256];
                StdRng::seed_from_u64(seed).fill_bytes(&mut data);
                arb::<R>(&mut Unstructured::new(&data)).expect("generator violates its own rule")
            })
            .collect()
    }

    #[test]
    fn generators_satisfy_rules() {
        samples::<Text>(500);
        samples::<FullText>(500);
        samples::<Link>(500);
        samples::<HexDigest>(500);
        samples::<Timestamp>(500);
    }

    #[test]
    fn exhausted_input_still_valid() {
        let mut u = Unstructured::new(&[]);
        assert!(arb::<Text>(&mut u).is_ok());
        assert!(arb::<Link>(&mut u).is_ok());
        assert!(arb::<HexDigest>(&mut u).is_ok());
        assert!(arb::<Timestamp>(&mut u).is_ok());
    }

    fn random_bytes(seed: u64) -> Vec<u8> {
        use rand::{Rng, SeedableRng, rngs::StdRng};
        let mut data = vec![0u8; 512];
        StdRng::seed_from_u64(seed).fill_bytes(&mut data);
        data
    }

    #[test]
    fn generated_vectors_are_sets() {
        for seed in 0..500 {
            let data = random_bytes(seed);
            let texts = Text::arb_vec(&mut Unstructured::new(&data)).expect("generation failed");
            assert!(check_unique(&texts).is_ok(), "{texts:?}");
            let bools: Vec<bool> =
                Set::arb(&mut Unstructured::new(&data)).expect("generation failed");
            assert!(bools.len() <= 2, "{bools:?}");
            let sgs: Vec<crate::enumerations::Sachgebiet> =
                Set::arb(&mut Unstructured::new(&data)).expect("generation failed");
            assert!(check_unique(&sgs).is_ok(), "{sgs:?}");
        }
    }

    #[test]
    fn duplicates_rejected() {
        #[derive(serde::Deserialize, Debug)]
        struct S {
            #[serde(deserialize_with = "Text::de_vec")]
            _texts: Vec<String>,
            #[serde(deserialize_with = "Set::de")]
            _nums: Vec<u8>,
        }
        assert!(serde_json::from_str::<S>(r#"{"_texts":["a","b"],"_nums":[1,2]}"#).is_ok());
        let e = serde_json::from_str::<S>(r#"{"_texts":["a","b","a"],"_nums":[]}"#)
            .expect_err("duplicate text");
        assert!(e.to_string().contains("entries 0 and 2"), "{e}");
        assert!(serde_json::from_str::<S>(r#"{"_texts":[],"_nums":[3,3]}"#).is_err());
    }

    #[test]
    fn keyed_set_compares_keys_only() {
        use crate::dokument::Zusammenfassungstupel;
        let tuple = |typ: &str, inhalt: &str| Zusammenfassungstupel {
            typ: typ.to_owned(),
            inhalt: inhalt.to_owned(),
        };
        assert!(KeyedSet::validate(&[tuple("a", "x"), tuple("b", "x")]).is_ok());
        assert!(KeyedSet::validate(&[tuple("a", "x"), tuple("a", "y")]).is_err());
        assert!(
            serde_json::from_str::<crate::Vorgang>(
                r#"{"api_id":"00000000-0000-0000-0000-000000000000","titel":"t","wahlperiode":1,
                "verfassungsaendernd":false,"typ":"sonstig","initiatoren":[],"stationen":[],
                "zusammenfassung":[{"typ":"a","inhalt":"x"},{"typ":"a","inhalt":"y"}]}"#
            )
            .is_err()
        );
        for seed in 0..200 {
            let data = random_bytes(seed);
            let lobby: Vec<crate::vorgang::Lobbyregistereintrag> =
                KeyedSet::arb(&mut Unstructured::new(&data)).expect("generation failed");
            assert!(KeyedSet::validate(&lobby).is_ok(), "{lobby:?}");
        }
    }

    #[test]
    fn dedup_keeps_first_occurrence_order() {
        assert_eq!(dedup(vec![3, 1, 3, 2, 1]), vec![3, 1, 2]);
    }

    #[test]
    fn validation_rejects() {
        assert!(Text::validate(&String::new()).is_err());
        assert!(Text::validate(&"a\0b".to_owned()).is_err());
        assert!(Text::validate(&"<b>x</b>".to_owned()).is_err());
        assert!(FullText::validate(&"<b>x</b>".to_owned()).is_ok());
        assert!(Link::validate(&"/relative".to_owned()).is_err());
        assert!(HexDigest::validate(&"abc".to_owned()).is_err());
        assert!(HexDigest::validate(&"g".repeat(40)).is_err());
        assert!(HexDigest::validate(&"a".repeat(40)).is_ok());
    }
}
