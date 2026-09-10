pub mod auth;
pub mod dokument;
pub mod enumerations;
pub mod headers;
pub mod misc;
pub mod params;
pub mod sitzung;
pub mod vorgang;

pub use auth::{APIKey, KeyTag};
pub use dokument::{
    Dokument, DokumentHash, DokumentOrApiId, FTMDokumentPages, HashWrapper, ZusammenfassungWrapper,
    Zusammenfassungstupel,
};
pub use enumerations::{
    APIScope, Doktyp, EnumerationNames, Parlament, Stationstyp, VgIdentTyp, Vorgangstyp,
};
pub use misc::{Autor, Gremium, TouchedByInner};
use serde::{Deserialize, Deserializer, Serialize, de};
pub use sitzung::{Sitzung, Top};
use std::str::FromStr;
use utoipa::ToSchema;
pub use vorgang::{Lobbyregistereintrag, Station, VgIdent, Vorgang};

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
    } else if ammonia::is_html(s) {
        Err(de::Error::custom(
            "String was recognized to contain html structure. 
                This might indicate cross-site scripting",
        ))
    } else if s.chars().any(|c| c == '\0') {
        Err(de::Error::custom(
            "This text contains '\\0', which is rejected due to being a C-String delimiter. 
If you believe, that the null char must be in your text, please file an issue",
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
fn de_string<'a, D: Deserializer<'a>>(deserializer: D) -> Result<String, D::Error> {
    let s = String::deserialize(deserializer)?;
    is_valid_string(&s)?;
    Ok(s)
}
fn de_link<'a, D: Deserializer<'a>>(deserializer: D) -> Result<String, D::Error> {
    let s = String::deserialize(deserializer)?;
    is_valid_string(&s)?;
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
