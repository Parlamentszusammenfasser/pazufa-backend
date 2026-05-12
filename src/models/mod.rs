pub mod dokument;
pub mod enumerations;
pub mod headers;
pub mod misc;
pub mod params;
pub mod sitzung;
pub mod vorgang;

pub use dokument::{Dokument, DokumentOrApiId};
pub use enumerations::{Doktyp, EnumerationNames, Parlament, Stationstyp, VgIdentTyp, Vorgangstyp};
pub use misc::{Autor, Gremium, KeyTag, TouchedByInner};
use serde::{Deserialize, Deserializer, Serialize, de};
pub use sitzung::{Sitzung, Top};
use utoipa::ToSchema;
pub use vorgang::{Lobbyregeintrag, Station, VgIdent, Vorgang};

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
    } else {
        Ok(s)
    }
}

fn de_string<'a, D: Deserializer<'a>>(deserializer: D) -> Result<String, D::Error> {
    let s = String::deserialize(deserializer)?;
    if s.is_empty() {
        Err(de::Error::custom(
            "Strings must not be empty. please fill them with TODO markers, with a sensible default or with processed content",
        ))
    } else if ammonia::is_html(&s) {
        Err(de::Error::custom(
            "String was recognized to contain html structure. 
                This might indicate cross-site scripting",
        ))
    } else {
        Ok(s)
    }
}

fn de_optstring<'a, D: Deserializer<'a>>(deserializer: D) -> Result<Option<String>, D::Error> {
    let os = <Option<String>>::deserialize(deserializer)?;
    if let Some(ref s) = os {
        if s.is_empty() {
            Err(de::Error::custom(
                "Strings must not be empty. Please pass `null` instead or omit the field",
            ))
        } else if ammonia::is_html(s) {
            Err(de::Error::custom(
                "String was recognized to contain html structure. 
                This might indicate cross-site scripting",
            ))
        } else {
            Ok(os)
        }
    } else {
        Ok(os)
    }
}

fn de_vecstring<'a, D: Deserializer<'a>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    let v = <Vec<String>>::deserialize(deserializer)?;
    for s in &v {
        if s.is_empty() {
            return Err(de::Error::custom(
                "Strings must not be empty. Please omit them.",
            ));
        } else if ammonia::is_html(s) {
            return Err(de::Error::custom(
                "String was recognized to contain html structure. 
                This might indicate cross-site scripting",
            ));
        }
    }
    Ok(v)
}
