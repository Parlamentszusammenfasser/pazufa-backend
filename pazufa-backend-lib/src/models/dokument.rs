use super::{de_full_text_string, de_hexstring, de_link, de_optstring, de_string, de_vecstring};
use crate::models::{
    self,
    enumerations::{HashStrategy, Mime},
    is_valid_string,
};
use serde::{Deserialize, Serialize, de, ser};
use utoipa::ToSchema;
use uuid::Uuid;

#[allow(non_snake_case, reason = "this reflects an external data structure")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct FTMDokumentPages {
    pub fileName: Option<String>,
    pub title: Option<String>,
    pub mimeType: Mime,
    pub parent: Option<String>,
    pub pdfHash: String,
    pub pages: Vec<FTMDokumentPages>,
    pub contentHash: String,
    pub author: String,
    pub generator: Option<String>,
    pub crawler: String,
    pub fileSize: usize,
    pub extension: Option<String>,
    pub encoding: String,
    pub bodyText: String,
    pub language: String,
    pub date: chrono::DateTime<chrono::Utc>,
    pub authoredAt: chrono::DateTime<chrono::Utc>,
    pub createdAt: chrono::DateTime<chrono::Utc>,
    pub modifiedAt: chrono::DateTime<chrono::Utc>,
    pub retrievedAt: chrono::DateTime<chrono::Utc>,
    pub publishedAt: chrono::DateTime<chrono::Utc>,
    pub country: String,
    pub summary: String,
    pub description: String,
    pub sourceUrl: String,
    pub publisher: String,
    pub publisherUrl: String,
    pub alephUrl: String,
    pub wikidataId: String,
    pub keywords: Vec<String>,
    pub topics: Vec<String>,
    pub program: String,
    pub namesMentioned: Vec<String>,
    pub peopleMentioned: Vec<String>,
    pub companiesMentioned: Vec<String>,
    pub ibanMentioned: Vec<String>,
    pub ipMentioned: Vec<String>,
    pub locationMentioned: Vec<String>,
    pub phoneMentioned: Vec<String>,
    pub emailMentioned: Vec<String>,
}

/// A typed partial summary of a document.
/// NOTE: Documents can only contain one summary of any type.
/// If on merge a different text is supplied for an existing type of summary, it replaces the old
/// one
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Zusammenfassungstupel {
    /// Type of summary, if the summary is made up of parts
    /// NOTE: there are some reserved type names:
    /// - `full`        means summary of the full document without origin info
    /// - `full-llm`    means summary of the full document, made by llm
    /// - `full-extern` means summary of the full document, taken from an external source
    ///
    /// You are free to add to these if required, just please stick to the ones available if
    /// possible
    #[schema(example = "Basisinformationen")]
    #[serde(deserialize_with = "de_string", default)]
    pub typ: String,
    /// Content of the summary part
    #[schema(
        example = "Das Gesetz zur Haarfärbeverordnung dient der Umsetzung der EU-Richtline 42/69420 zur Schuppenfreiheit bei Eigenschaftsänderlichen Haarmodifikationen vor..."
    )]
    #[serde(deserialize_with = "de_string", default)]
    pub inhalt: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct DokumentHash {
    /// Hash value as string of hexadecimal octets
    #[serde(deserialize_with = "de_hexstring")]
    pub value: String,

    /// The strategy used to compute the hash.
    /// sha256+text denotes that not the raw file, but the _exact_ volltext field
    /// of the document was hashed.
    /// All text must be utf-8, remain stable under subsequent extraction,
    /// and mime must be set to text/plain or text/html.
    pub strategy: HashStrategy,

    /// The mime of the hashed content.
    /// If sha256/1+bytes was used, must be application/pdf;
    /// If sha256+text was used, must be text/plain
    pub mime: Mime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
#[serde(remote = "Self")]
pub enum HashWrapper {
    Legacy(String),
    Future(Vec<DokumentHash>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
#[serde(remote = "Self")]
pub enum ZusammenfassungWrapper {
    Legacy(String),
    Future(Vec<Zusammenfassungstupel>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
#[serde(remote = "Self")]
pub enum DokumentOrApiId {
    Dokument(Box<Dokument>),
    ApiId(Uuid),
}
impl From<models::Dokument> for DokumentOrApiId {
    fn from(value: models::Dokument) -> Self {
        Self::Dokument(Box::new(value))
    }
}
impl From<Uuid> for DokumentOrApiId {
    fn from(value: Uuid) -> Self {
        Self::ApiId(value)
    }
}
/// A document. Could be a protocol, a Drucksache, a statement, ...
/// For different objects, only certain properties are optional, which is a known design flaw.
/// A Stellungnahme must contain `meinung`, a Gesetzentwurf contains `Vorwort`.
/// Since documents can become quite large, they are returned by the server as just their `api_id` in lists
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(remote = "Self")]
pub struct Dokument {
    /// optional, here for future references. Is generated by the server. If you set it use UUID version 5 with your collector id as namespace
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_id: Option<uuid::Uuid>,

    /// list of scraper uuids / key database ids that have touched this object
    #[serde(skip_serializing_if = "Vec::is_empty", default = "Vec::new")]
    pub touched_by: Vec<models::TouchedByInner>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(deserialize_with = "de_optstring", default)]
    pub drucksnr: Option<String>,

    pub typ: models::Doktyp,

    /// Official Title of the Document
    #[serde(deserialize_with = "de_string")]
    pub titel: String,

    /// Brief, Summarized, layman-readable title of the document
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(deserialize_with = "de_optstring", default)]
    pub kurztitel: Option<String>,

    /// Preamble, synopsys or statement of intent
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(deserialize_with = "de_optstring", default)]
    pub vorwort: Option<String>,

    /// Full text of the document, possibly normalized
    #[serde(rename = "volltext")]
    #[serde(deserialize_with = "de_full_text_string")]
    pub volltext: String,

    /// Summary of the document's contents
    pub zusammenfassung: Option<ZusammenfassungWrapper>,

    /// Protocol of the session on 7.3., created on 8.3. *modified on 9.3*.
    pub zp_modifiziert: chrono::DateTime<chrono::Utc>,

    /// Protocol of the *session on 7.3.*, created on 8.3. modified on 9.3.
    pub zp_referenz: chrono::DateTime<chrono::Utc>,

    /// Protocol of the session on 7.3., *created on 8.3.* modified on 9.3.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zp_erstellt: Option<chrono::DateTime<chrono::Utc>>,

    #[schema(format = "uri")]
    #[serde(deserialize_with = "de_link")]
    pub link: String,

    /// corresponds to sha256+bytes, here for backwards compatibility
    pub hash: HashWrapper,

    /// General opinion of the gremium about the Document
    /// Stellungnahme:       1=very bad, 5=very good
    /// Beschlussempfehlung: 1=rejection recommended, 2-4=accept in changed form, 5=acceptence recommended
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(minimum = 1, maximum = 5)]
    pub meinung: Option<u8>,

    /// Keywords associated with this document
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(deserialize_with = "de_vecstring", default = "Vec::new")]
    pub schlagworte: Vec<String>,

    /// List of authors of the document. Could be a person, organisation or a parliamentary body
    pub autoren: Vec<models::Autor>,

    /// If a document contains more than one semantically closed document (x opinions in a collected file for example)
    /// this denotes the index of the sub-file in question. They can potentially share a hash.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subdoc_id: Option<u16>,
}
impl std::default::Default for Dokument {
    fn default() -> Self {
        Self {
            api_id: None,
            typ: models::Doktyp::Entwurf,
            link: String::new(),
            hash: HashWrapper::Legacy(String::new()),
            titel: String::new(),
            vorwort: None,
            meinung: None,
            autoren: vec![],
            drucksnr: None,
            volltext: String::new(),
            kurztitel: None,
            touched_by: vec![],
            zp_referenz: chrono::Utc::now(),
            zp_erstellt: None,
            schlagworte: vec![],
            zp_modifiziert: chrono::Utc::now(),
            zusammenfassung: None,
            subdoc_id: None,
        }
    }
}

impl<'de> Deserialize<'de> for Dokument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let unchecked = Dokument::deserialize(deserializer)?;
        if let Some(meinung) = &unchecked.meinung
            && !(1..=5).contains(meinung)
        {
            return Err(de::Error::custom(
                "dokument.meinung must be between 1 and 5",
            ));
        }
        Ok(unchecked)
    }
}
impl Serialize for Dokument {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        Dokument::serialize(self, serializer)
    }
}
impl std::fmt::Display for Dokument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![
            // Skipping api_id in query parameter serialization

            // Skipping touched_by in query parameter serialization
            self.drucksnr
                .as_ref()
                .map(|drucksnr| ["drucksnr", drucksnr].join(",")),
            // Skipping typ in query parameter serialization
            Some("titel".to_string()),
            Some(self.titel.clone()),
            self.kurztitel
                .as_ref()
                .map(|kurztitel| ["kurztitel", kurztitel].join(",")),
            self.vorwort
                .as_ref()
                .map(|vorwort| ["vorwort", vorwort].join(",")),
            Some("volltext".to_string()),
            Some(self.volltext.clone()),
            self.zusammenfassung.as_ref().map(|zusammenfassung| {
                ["zusammenfassung", &format!("{zusammenfassung:?}")].join(",")
            }),
            // Skipping zp_modifiziert in query parameter serialization

            // Skipping zp_referenz in query parameter serialization

            // Skipping zp_erstellt in query parameter serialization
            Some("link".to_string()),
            Some(self.link.clone()),
            Some("hash".to_string()),
            Some(format!("{:?}", self.hash)),
            self.meinung
                .as_ref()
                .map(|meinung| ["meinung".to_string(), meinung.to_string()].join(",")),
            Some(["schlagworte".to_string(), self.schlagworte.join(",")].join(",")),
            // Skipping autoren in query parameter serialization
        ];

        write!(
            f,
            "{}",
            params.into_iter().flatten().collect::<Vec<_>>().join(",")
        )
    }
}
impl<'de> Deserialize<'de> for DokumentOrApiId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        if let Ok(api_id) = Uuid::deserialize(value.clone()) {
            return Ok(Self::ApiId(api_id));
        }
        match Dokument::deserialize(value.clone()) {
            Ok(d) => Ok(Self::Dokument(Box::new(d))),
            Err(e) => Err(serde::de::Error::custom(format!(
                "Unable to parse value either as document or as API ID
Document Parsing Error:\n{e}"
            ))),
        }
    }
}
impl Serialize for DokumentOrApiId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for HashWrapper {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;

        #[allow(
            unused,
            reason = "I just want to keep these around to hold the parsing errors"
        )]
        let mut hl_parsing_err = None;
        #[allow(
            unused,
            reason = "I just want to keep these around to hold the parsing errors"
        )]
        let mut str_parsing_err = None;

        match <Vec<DokumentHash>>::deserialize(value.clone()) {
            Ok(hashlist) => {
                if hashlist.is_empty() {
                    return Err(serde::de::Error::custom(
                        "Hashes cannot be an empty list; At least one Hash is required for a complete Document",
                    ));
                }
                if hashlist.iter().all(|x| {
                    (40usize..=64).contains(&x.value.len())
                        && is_valid_string::<D::Error>(&x.value).is_ok()
                        && x.value.chars().all(|c| c.is_ascii_hexdigit())
                        && (x.strategy != HashStrategy::Sha256Text
                            || matches!(x.mime, Mime::TextHtml | Mime::TextPlain))
                }) {
                    return Ok(Self::Future(hashlist));
                }
                return Err(serde::de::Error::custom(
                    "Some Hash Value is not: 
                - in the applicable length of hexadecimal chars: [40, 64]
                - a valid string
                - a string of hexadecimal characters
                - or it failed the mime/hash sanity check",
                ));
            }
            Err(e) => hl_parsing_err = Some(e.to_string()),
        }
        match String::deserialize(value.clone()) {
            Ok(hash) => {
                if !(40usize..=64).contains(&hash.len()) {
                    return Err(serde::de::Error::custom(format!(
                        "Legacy Hash of length {} is not in length range [40; 64]",
                        hash.len()
                    )));
                }
                if is_valid_string::<D::Error>(&hash).is_err() {
                    return Err(serde::de::Error::custom(
                        "Legacy Hash is not a valid string in our sense",
                    ));
                }
                if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Err(serde::de::Error::custom(
                        "Legacy Hash must be a string of hexadecimal characters",
                    ));
                }
                return Ok(Self::Legacy(hash));
            }
            Err(e) => str_parsing_err = Some(e.to_string()),
        }
        if let Some(hl) = hl_parsing_err
            && let Some(st) = str_parsing_err
        {
            Err(serde::de::Error::custom(format!(
                "Unable to parse value either as String or HashList.
                Hashlist Parsing Error: {hl}
                String Parsing Error:   {st}"
            )))
        } else {
            unreachable!(
                "If Hash list and String deserialization failed, both error thingies should contain errors"
            )
        }
    }
}

impl Serialize for HashWrapper {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for ZusammenfassungWrapper {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        if let Ok(summ) = String::deserialize(value.clone()) {
            return Ok(Self::Legacy(summ));
        }

        match <Vec<Zusammenfassungstupel>>::deserialize(value.clone()) {
            Ok(tupellist) => Ok(Self::Future(tupellist)),
            Err(e) => Err(serde::de::Error::custom(format!(
                "Unable to parse value either as String or Vec<Zusammenfassungstupel>.
                Zusammenfassungstupel: Parsing Error:\n{e}"
            ))),
        }
    }
}

impl Serialize for ZusammenfassungWrapper {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        Self::serialize(self, serializer)
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "in test cases, love and war all things are allowed"
)]
mod dokument_deserialize {
    #[test]
    fn deserialize_dokument() {
        let s: super::Dokument =
            serde_json::from_str(include_str!("./../../tests/serialization/dokument.json"))
                .expect("Expected flawless serialization");
        assert_eq!(s.link, "string");
    }
    #[test]
    fn deserialize_dokument_invar_fail() {
        let _e = serde_json::from_str::<super::Dokument>(include_str!(
            "./../../tests/serialization/dokument-invar.json"
        ))
        .expect_err("Expected a validation failure");
    }
    #[test]
    fn deserialize_enum() {
        let s: super::DokumentOrApiId =
            serde_json::from_str(include_str!("./../../tests/serialization/dokument.json"))
                .expect("Expected flawless serialization");
        assert!(matches!(s, super::DokumentOrApiId::Dokument(_)));
    }

    #[test]
    fn test_document_rejects_xss_except_volltext_success() {
        let dokument = r#"
        {"titel": "kackstift", "typ": "entwurf", "volltext": "Peter Maffay <peter-maffay@roflkopter.lol> lorem ipsum dolor set amet",
        "zp_referenz": "2026-01-01T10:10:10Z", "zp_modifiziert": "2026-01-01T10:10:10Z",
        "link": "https://example.com", "hash": "d8af09d7f56a976d5f9a7d659fad65fd8af09d7f56a976d5f9a7d659fad65f", 
        "autoren": []}"#;
        let _deser: super::Dokument = serde_json::from_str(dokument).expect("Success");
    }
    #[test]
    fn test_document_fail_xss_html_in_title() {
        let dokument = r#"
        {"titel": "</kackstift>", "typ": "entwurf", "volltext": "Peter Maffay <peter-maffay@roflkopter.lol> lorem ipsum dolor set amet",
        "zp_referenz": "2026-01-01T10:10:10Z", "zp_modifiziert": "2026-01-01T10:10:10Z",
        "link": "https://example.com", "hash": "d8af09d7f56a976d5f9a7d659fad65fd8af09d7f56a976d5f9a7d659fad65f", 
        "autoren": []}"#;
        let deser: Result<super::Dokument, _> = serde_json::from_str(dokument);
        deser.expect_err("Success");
    }
    #[test]
    fn issue110_nullbyte() {
        let dokument = r#"{"titel": "kackstift", "typ": "entwurf", 
        "volltext": "Peter Maffay <peter-maffay@roflkopter.lol> lorem ipsum \u0000 dolor set amet", 
        "zp_referenz": "2026-01-01T10:10:10Z", "zp_modifiziert": "2026-01-01T10:10:10Z",
        "link": "https://example.com", "hash": "d8af09d7f56a976d5f9a7d659fad65fd8af09d7f56a976d5f9a7d659fad65f", 
        "autoren": []}"#;
        let deser: Result<super::Dokument, _> = serde_json::from_str(dokument);
        deser.expect_err("Success");

        let dokument = r#"{"titel": "kackstift\u0000 karla", "typ": "entwurf", 
        "volltext": "Petr Maffay <peter-maffay@roflkopter.lol> lorem ipsum dolor set amet", 
        "zp_referenz": "2026-01-01T10:10:10Z", "zp_modifiziert": "2026-01-01T10:10:10Z",
        "link": "https://example.com", "hash": "d8af09d7f56a976d5f9a7d659fad65fd8af09d7f56a976d5f9a7d659fad65f", 
        "autoren": []}"#;
        let deser: Result<super::Dokument, _> = serde_json::from_str(dokument);
        deser.expect_err("Success");

        // reproducer document
        let dokument = r#"{
      "titel": "NULDOC",
      "typ": "preparl-entwurf",
      "volltext": "Vorher \u0000 Nachher",
      "link": "https://example.com",
      "hash": "9as8duf9a8dsfjij9j",
      "zp_referenz": "2024-01-01T00:00:00+00:00",
      "zp_modifiziert": "2024-01-01T00:00:00+00:00",
      "autoren": []
    }"#;
        let deser: Result<super::Dokument, _> = serde_json::from_str(dokument);
        deser.expect_err("Success");
    }
}
