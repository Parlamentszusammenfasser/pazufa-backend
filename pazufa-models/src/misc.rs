use crate::constraint::{Link, Text};
use crate::{self as models};
use arbitrary::Arbitrary;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use utoipa::ToSchema;

#[derive(Arbitrary, Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TouchedByInner {
    /// uuid of the scraper that touched this object
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scraper_id: Option<uuid::Uuid>,

    /// Key hash of the scraper that touched the object
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(deserialize_with = "Text::de_opt")]
    #[arbitrary(with = Text::arb_opt)]
    pub key: Option<String>,
}

/// A body in which decisions can be made: committees, plenary halls, cabinett, peoples, ...
#[derive(Arbitrary, Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(remote = "Self")]
pub struct Gremium {
    pub parlament: models::Parlament,

    pub wahlperiode: u16,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(format = "uri")]
    #[serde(deserialize_with = "Link::de_opt", default)]
    #[arbitrary(with = Link::arb_opt)]
    pub link: Option<String>,

    /// Name of the body. 'plenum', 'regierung', 'volk' are reserved
    #[serde(deserialize_with = "Text::de")]
    #[arbitrary(with = Text::arb)]
    pub name: String,
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

impl Default for Gremium {
    fn default() -> Self {
        Self {
            name: "DEFAULT".to_owned(),
            link: None,
            parlament: crate::Parlament::Bt,
            wahlperiode: 27,
        }
    }
}

/// Person or organisation in some function. e.g.:
/// - authors of a statement,
/// - expert at a hearing,
/// - initiator of a Vorgang
/// - authoring organisations of documents
#[derive(Arbitrary, Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(remote = "Self")]
pub struct Autor {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(deserialize_with = "Text::de_opt", default)]
    #[arbitrary(with = Text::arb_opt)]
    pub person: Option<String>,

    #[serde(deserialize_with = "Text::de")]
    #[arbitrary(with = Text::arb)]
    pub organisation: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(deserialize_with = "Text::de_opt", default)]
    #[arbitrary(with = Text::arb_opt)]
    pub fachgebiet: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(format = "uri")]
    #[serde(deserialize_with = "Link::de_opt", default)]
    #[arbitrary(with = Link::arb_opt)]
    pub lobbyregister: Option<String>,
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
