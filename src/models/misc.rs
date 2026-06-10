use super::{de_optlink, de_optstring, de_string};
use crate::models;
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, str::FromStr};
use utoipa::ToSchema;

/// A uniquely identifying string that is generated from key data within the database
#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize, ToSchema)]
pub struct KeyTag(pub String);

impl std::convert::From<String> for KeyTag {
    fn from(x: String) -> Self {
        KeyTag(x)
    }
}

impl std::fmt::Display for KeyTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for KeyTag {
    type Err = std::string::ParseError;
    fn from_str(x: &str) -> std::result::Result<Self, Self::Err> {
        std::result::Result::Ok(KeyTag(x.to_string()))
    }
}

impl std::convert::From<KeyTag> for String {
    fn from(x: KeyTag) -> Self {
        x.0
    }
}

impl std::ops::Deref for KeyTag {
    type Target = String;
    fn deref(&self) -> &String {
        &self.0
    }
}

impl std::ops::DerefMut for KeyTag {
    fn deref_mut(&mut self) -> &mut String {
        &mut self.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TouchedByInner {
    /// uuid of the scraper that touched this object
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scraper_id: Option<uuid::Uuid>,

    /// Key hash of the scraper that touched the object
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(deserialize_with = "de_optstring")]
    pub key: Option<String>,
}

/// Ein Gremium in dem Entscheidungen getroffen werden können. z.B: Ausschüsse, Plenum, Kabinett, Volk, ...
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(remote = "Self")]
pub struct Gremium {
    pub parlament: models::Parlament,

    pub wahlperiode: u16,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(format = "uri")]
    #[serde(deserialize_with = "de_optlink", default)]
    pub link: Option<String>,

    /// Name des betreffenden Gremiums. 'plenum', 'regierung', 'volk' sind reservierte namen
    #[serde(deserialize_with = "de_string")]
    pub name: String,
}
impl<'de> Deserialize<'de> for Gremium {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let unchecked = Gremium::deserialize(deserializer)?;
        if let Some(link) = &unchecked.link {
            if let Err(e) = axum::http::Uri::from_str(link) {
                Err(de::Error::custom(e.to_string()))
            } else {
                Ok(unchecked)
            }
        } else {
            Ok(unchecked)
        }
    }
}
impl Serialize for Gremium {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        Gremium::serialize(self, serializer)
    }
}

impl std::fmt::Display for Gremium {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![
            // Skipping parlament in query parameter serialization
            Some("wahlperiode".to_string()),
            Some(self.wahlperiode.to_string()),
            self.link.as_ref().map(|link| ["link", link].join(",")),
            Some("name".to_string()),
            Some(self.name.clone()),
        ];

        write!(
            f,
            "{}",
            params.into_iter().flatten().collect::<Vec<_>>().join(",")
        )
    }
}

/// Eine Person oder Organisation, die eine bestimmte Funktion übernommen hat. Z.B: Autor einer Stellungnahme, Experte bei einer Anhörung, Initiator eines Vorgangs.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(remote = "Self")]
pub struct Autor {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(deserialize_with = "de_optstring", default)]
    pub person: Option<String>,

    #[serde(deserialize_with = "de_string")]
    pub organisation: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(deserialize_with = "de_optstring", default)]
    pub fachgebiet: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(format = "uri")]
    #[serde(deserialize_with = "de_optstring", default)]
    pub lobbyregister: Option<String>,
}
use serde::{de, ser};
impl<'de> Deserialize<'de> for Autor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let unchecked = Autor::deserialize(deserializer)?;
        if let Some(lobbyreg) = &unchecked.lobbyregister {
            if let Err(e) = axum::http::Uri::from_str(lobbyreg) {
                Err(de::Error::custom(e.to_string()))
            } else {
                Ok(unchecked)
            }
        } else {
            Ok(unchecked)
        }
    }
}
impl Serialize for Autor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        Autor::serialize(self, serializer)
    }
}

impl std::fmt::Display for Autor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![
            self.person
                .as_ref()
                .map(|person| ["person", person].join(",")),
            Some("organisation".to_string()),
            Some(self.organisation.clone()),
            self.fachgebiet
                .as_ref()
                .map(|fachgebiet| ["fachgebiet", fachgebiet].join(",")),
            self.lobbyregister
                .as_ref()
                .map(|lobbyregister| ["lobbyregister", lobbyregister].join(",")),
        ];

        write!(
            f,
            "{}",
            params.into_iter().flatten().collect::<Vec<_>>().join(",")
        )
    }
}
impl Eq for Gremium {}
impl PartialOrd for Gremium {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(std::cmp::Ord::cmp(self, other))
    }
}
impl Ord for Gremium {
    fn cmp(&self, other: &Self) -> Ordering {
        self.parlament
            .cmp(&other.parlament)
            .then(self.wahlperiode.cmp(&other.wahlperiode))
            .then(self.name.cmp(&other.name))
    }
}

impl PartialEq for Autor {
    fn eq(&self, other: &Self) -> bool {
        self.organisation == other.organisation && self.person == other.person
    }
}
impl Eq for Autor {}
impl PartialOrd for Autor {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(std::cmp::Ord::cmp(self, other))
    }
}
impl Ord for Autor {
    fn cmp(&self, other: &Self) -> Ordering {
        self.organisation
            .cmp(&other.organisation)
            .then(self.person.cmp(&other.person))
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn deser_autor_without_person() {
        let input = r#"{"organisation": "Landesregierung"}"#;
        let output: Result<super::Autor, _> = serde_json::from_str(input);
        output.expect("Autor can live without a person");
    }
}
