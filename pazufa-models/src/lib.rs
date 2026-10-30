#![cfg_attr(
    test,
    allow(clippy::expect_used, reason = "all is fair in love, war and testing")
)]
#![cfg_attr(
    test,
    allow(
        clippy::arithmetic_side_effects,
        reason = "all is fair in love, war and testing"
    )
)]
#![cfg_attr(
    test,
    allow(clippy::unwrap_used, reason = "all is fair in love, war and testing")
)]

mod arbitrary;
pub mod auth;
pub mod dokument;
pub mod enumerations;
pub mod error;
pub mod headers;
pub mod misc;
pub mod params;
pub mod sitzung;
pub mod vorgang;

pub use auth::{APIKey, APIKeyGenerator, APIKeyMetadata, HashAlgorithm, KeyTag};
pub use dokument::{
    Dokument, DokumentHash, DokumentOrApiId, FTMDokumentPages, HashWrapper, ZusammenfassungWrapper,
    Zusammenfassungstupel,
};
pub use enumerations::{
    APIScope, Doktyp, EnumerationNames, Parlament, Stationstyp, VgIdentTyp, Vorgangstyp,
};
pub use misc::{Autor, Gremium, TouchedByInner};
pub use sitzung::{Sitzung, Top};
pub use vorgang::{Lobbyregistereintrag, Station, VgIdent, Vorgang};

use chrono::{DateTime, DurationRound, Utc};
use serde::{Deserialize, Deserializer, Serialize, de};
use std::str::FromStr;
use utoipa::ToSchema;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ReplacementPutRequest<T: ToSchema> {
    #[serde(rename = "objects")]
    pub objects: Vec<T>,

    #[serde(rename = "replacing")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub replacing: Vec<ReplacementPutRequestInner<T>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ReplacementPutRequestInner<T: ToSchema> {
    #[serde(rename = "values")]
    pub values: Vec<T>,

    /// This object is replaced by the object with index {} in the 'objects' list above. 0-Based indexing.
    #[serde(rename = "replaced_by")]
    pub replaced_by: u32,
}
fn de_full_text_string<'a, D: Deserializer<'a>>(deserializer: D) -> Result<String, D::Error> {
    let s = String::deserialize(deserializer)?;
    if s.is_empty() {
        Err(de::Error::custom(
            "Strings must not be empty. please fill them with TODO markers, with a sensible default or with processed content",
        ))
    } else if s.chars().any(|c| c == '\0') {
        Err(de::Error::custom(
            "This text contains '\\0'. Please no shenanigans, law texts are boring",
        ))
    } else {
        Ok(s)
    }
}
fn is_valid_string<E: serde::de::Error>(s: &str) -> Result<(), E> {
    if s.is_empty() {
        Err(de::Error::custom(
            "Strings must not be empty. please fill them with TODO markers, with a sensible default or with processed content",
        ))
    } else if s.chars().any(|c| c == '\0') {
        Err(de::Error::custom(
            "This text contains '\\0', which is rejected due to being the C-String terminator",
        ))
    } else if ammonia::is_html(s) {
        Err(de::Error::custom(
            "String was recognized to contain html structure. 
                This might indicate cross-site scripting",
        ))
    } else {
        Ok(())
    }
}
fn is_valid_link<E: serde::de::Error>(s: &str) -> Result<(), E> {
    let Ok(uri) = axum::http::Uri::from_str(s) else {
        return Err(de::Error::custom("This vector should only contain links"));
    };
    if uri.scheme().is_none() || uri.authority().is_none() {
        return Err(de::Error::custom(
            "This vector should only contain links with a scheme (like https)",
        ));
    }
    Ok(())
}
fn de_hexstring<'a, D: Deserializer<'a>>(deserializer: D) -> Result<String, D::Error> {
    let s = String::deserialize(deserializer)?;
    is_valid_string(&s)?;
    if s.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(s)
    } else {
        Err(de::Error::custom(
            "String contains non-hexadecimal digit characters",
        ))
    }
}
fn is_valid_datetime<E: serde::de::Error>(dt: chrono::DateTime<chrono::Utc>) -> Result<(), E> {
    let baseline_time = chrono::DateTime::parse_from_rfc3339("0000-01-01T00:00:00Z")
        .map_err(|_| unreachable!())?
        .to_utc();
    let end_time = chrono::DateTime::parse_from_rfc3339("9999-01-01T00:00:00Z")
        .map_err(|_| unreachable!())?
        .to_utc();
    if dt < baseline_time {
        Err(de::Error::custom("DateTime is too early"))
    } else if dt >= end_time {
        Err(de::Error::custom("DateTime is too late"))
    } else {
        Ok(())
    }
}

fn de_optdatetime<'a, D: Deserializer<'a>>(
    deserializer: D,
) -> Result<Option<chrono::DateTime<chrono::Utc>>, D::Error> {
    let s = Option::<chrono::DateTime<chrono::Utc>>::deserialize(deserializer)?;
    if let Some(s) = s {
        is_valid_datetime(s)?;
    }
    Ok(s)
}
fn de_datetime<'a, D: Deserializer<'a>>(
    deserializer: D,
) -> Result<chrono::DateTime<chrono::Utc>, D::Error> {
    let s = chrono::DateTime::<chrono::Utc>::deserialize(deserializer)?;
    is_valid_datetime(s)?;
    Ok(s)
}
fn de_string<'a, D: Deserializer<'a>>(deserializer: D) -> Result<String, D::Error> {
    let s = String::deserialize(deserializer)?;
    is_valid_string(&s)?;
    Ok(s)
}
fn de_link<'a, D: Deserializer<'a>>(deserializer: D) -> Result<String, D::Error> {
    let s = String::deserialize(deserializer)?;
    is_valid_string(&s)?;
    is_valid_link(&s)?;
    Ok(s)
}

fn de_optstring<'a, D: Deserializer<'a>>(deserializer: D) -> Result<Option<String>, D::Error> {
    let os = <Option<String>>::deserialize(deserializer)?;
    if let Some(ref s) = os {
        is_valid_string(s)?;
        Ok(os)
    } else {
        Ok(os)
    }
}
fn de_optlink<'a, D: Deserializer<'a>>(deserializer: D) -> Result<Option<String>, D::Error> {
    let os = <Option<String>>::deserialize(deserializer)?;
    if let Some(ref s) = os {
        is_valid_string(s)?;
        is_valid_link(s)?;
    }
    Ok(os)
}

fn de_vecstring<'a, D: Deserializer<'a>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    let v = <Vec<String>>::deserialize(deserializer)?;
    for s in &v {
        is_valid_string(s)?;
    }
    Ok(v)
}
fn de_veclinks<'a, D: Deserializer<'a>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    let v = <Vec<String>>::deserialize(deserializer)?;
    for s in &v {
        is_valid_string(s)?;
        is_valid_link(s)?;
    }
    Ok(v)
}

/// Helper Trait that allows me to compare objects (vorgang, dokument, ...)
/// that are stored and re-fetched with whatever precision where only the
/// very very margins differ by a few nanosecs.
/// Using this trait the thing can round its dates to a precision of 1 second
/// We do not really need more
pub trait RoundTimestamp: Clone {
    #[must_use]
    fn with_round_timestamps(&self) -> Self;
}

fn round_to_second(datetime: DateTime<Utc>) -> DateTime<Utc> {
    if let Ok(datetime) = datetime.duration_round(const { chrono::Duration::seconds(1) }) {
        datetime
    } else {
        let mut unix_time = datetime.timestamp();
        if datetime.timestamp_subsec_millis() >= 500 {
            unix_time = unix_time.saturating_add(1);
        }
        let Some(datetime) = DateTime::from_timestamp(unix_time, 0) else {
            unreachable!("Only returns None, if `nsecs` is greater then 999,999,999.");
        };
        datetime
    }
}

impl RoundTimestamp for Dokument {
    fn with_round_timestamps(&self) -> Self {
        Self {
            zp_referenz: round_to_second(self.zp_referenz),
            zp_erstellt: self.zp_erstellt.map(round_to_second),
            zp_modifiziert: round_to_second(self.zp_modifiziert),
            ..self.clone()
        }
    }
}

impl RoundTimestamp for Station {
    fn with_round_timestamps(&self) -> Self {
        Self {
            zp_modifiziert: self.zp_modifiziert.map(round_to_second),
            zp_start: round_to_second(self.zp_start),
            stellungnahmen: self
                .stellungnahmen
                .iter()
                .map(|sn| match sn {
                    DokumentOrApiId::Dokument(d) => {
                        DokumentOrApiId::Dokument(Box::new(d.with_round_timestamps()))
                    }
                    DokumentOrApiId::ApiId(x) => DokumentOrApiId::ApiId(*x),
                })
                .collect(),
            dokumente: self
                .dokumente
                .iter()
                .map(|sn| match sn {
                    DokumentOrApiId::Dokument(d) => {
                        DokumentOrApiId::Dokument(Box::new(d.with_round_timestamps()))
                    }
                    DokumentOrApiId::ApiId(x) => DokumentOrApiId::ApiId(*x),
                })
                .collect(),

            ..self.clone()
        }
    }
}

impl RoundTimestamp for Vorgang {
    fn with_round_timestamps(&self) -> Self {
        Self {
            stationen: self
                .stationen
                .iter()
                .map(RoundTimestamp::with_round_timestamps)
                .collect(),
            ..self.clone()
        }
    }
}
impl RoundTimestamp for Sitzung {
    fn with_round_timestamps(&self) -> Self {
        Self {
            dokumente: self
                .dokumente
                .iter()
                .map(|sn| match sn {
                    DokumentOrApiId::Dokument(d) => {
                        DokumentOrApiId::Dokument(Box::new(d.with_round_timestamps()))
                    }
                    DokumentOrApiId::ApiId(x) => DokumentOrApiId::ApiId(*x),
                })
                .collect(),
            termin: round_to_second(self.termin),
            ..self.clone()
        }
    }
}
