use crate::RoundTimestamp;
use crate::constraint::{HexDigest, KeyedSet, Text};

use crate::auth::{APIKey, APIKeyCreate, APIKeyGenerator};
use crate::dokument::{
    Dokument, DokumentHash, DokumentOrApiId, HashWrapper, ZusammenfassungWrapper,
    Zusammenfassungstupel,
};
use crate::enumerations::{HashStrategy, Mime, Stationstyp};
use crate::misc::{Autor, Gremium};
use crate::params::KalGetQueryParams;
use crate::sitzung::{Sitzung, Top};
use crate::vorgang::{Lobbyregistereintrag, Station, Vorgang};

use serde::{Deserialize, Serialize, de, ser};
use std::str::FromStr;
use uuid::Uuid;

impl<'de> Deserialize<'de> for APIKeyCreate {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let unchecked = Self::deserialize(deserializer)?;
        // check that the max length of the keytag name is <= 10
        if let Some(kt) = &unchecked.keytag_prefix
            && kt.len() > 10
        {
            return Err(serde::de::Error::custom(
                "Error: Supplied Keytag is too long (>10 ascii characters)",
            ));
        }
        Ok(unchecked)
    }
}

impl Serialize for APIKeyCreate {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        Self::serialize(self, serializer)
    }
}
impl<'de, T: APIKeyGenerator> Deserialize<'de> for APIKey<T> {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let unchecked = String::deserialize(deserializer)?;
        let out = Self::try_from(unchecked)
            .map_err(|e| de::Error::custom(format!("Error deserializing: {e}")))?;
        Ok(out)
    }
}

impl<'de> Deserialize<'de> for Vorgang {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let unchecked = Self::deserialize(deserializer)?;
        // check that no two stations have the exact same zp_start, rounded
        let unchecked = unchecked.with_round_timestamps();
        let mut fails = vec![];
        for i in 0..unchecked.stationen.len() {
            #[allow(
                clippy::arithmetic_side_effects,
                reason = "cannot be larger than .len()"
            )]
            for j in (i + 1)..unchecked.stationen.len() {
                let s1 = &unchecked.stationen[i];
                let s2 = &unchecked.stationen[j];
                if s1.typ != s2.typ
                    && s1.zp_start == s2.zp_start
                    && s1.meta_tiebreaker == s2.meta_tiebreaker
                {
                    fails.push(format!(
                        "({}/{}) and ({}/{})",
                        unchecked.stationen[i].typ,
                        unchecked.stationen[i].zp_start,
                        unchecked.stationen[j].typ,
                        unchecked.stationen[j].zp_start,
                    ));
                }
                if let Some(a) = s1.api_id
                    && let Some(b) = s2.api_id
                    && a == b
                {
                    return Err(de::Error::custom(format!(
                        "Two API IDs are equal for different stations in the input data: {a}"
                    )));
                }
            }
        }
        if fails.is_empty() {
            Ok(unchecked)
        } else {
            Err(de::Error::custom(format!("Error: Some stations of different type have the exact same `zp_start` and `meta_tiebreaker` value.: {fails:?}\n
            This is not allowed because this value is used for imposing a total order on the stations for track validation.
                        Please modify these values to make clear which station comes first. For more details see https://codeberg.org/PaZuFa/pazufa-backend/issues/57.")))
        }
    }
}
impl Serialize for Vorgang {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for Station {
    // the trojanergefahr-constraint is skipped since the whole field is skipped.
    // it will be ignored anyways in 0.2.4
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let unchecked = Station::deserialize(deserializer)?;
        // custom validation: Dokumente should be objects at deserialization time
        let api_ids: Vec<_> = unchecked
            .dokumente
            .iter()
            .enumerate()
            .filter_map(|(index, thing)| match thing {
                DokumentOrApiId::ApiId(i) => Some((index, i)),
                DokumentOrApiId::Dokument(_) => None,
            })
            .collect();
        if !api_ids.is_empty() {
            return Err(de::Error::custom(format!(
                "Documents Cannot be api ids at upload time, only full objects. The following entries of station.dokumente were api ids(index, api_id): {api_ids:?}"
            )));
        }
        // same for stellungnahmen
        let api_ids: Vec<_> = unchecked
            .stellungnahmen
            .iter()
            .enumerate()
            .filter_map(|(index, thing)| match thing {
                DokumentOrApiId::ApiId(i) => Some((index, i)),
                DokumentOrApiId::Dokument(_) => None,
            })
            .collect();
        if !api_ids.is_empty() {
            return Err(de::Error::custom(format!(
                "Stellungnahmen cannot be api ids at upload time, only full objects. The following entries of station.stellungnahmen were api ids(index, api_id): {api_ids:?}"
            )));
        }

        // constraint: gremium_federf can only be SOME for parl-ausschber
        if unchecked.gremium_federf.is_some() && unchecked.typ != Stationstyp::ParlAusschber {
            return Err(de::Error::custom(
                "gremium_federf only makes sense in stations of type parl-ausschber",
            ));
        }
        // constraint: zp_start cannot be more than zp_modifiziert
        if let Some(zp_mod) = unchecked.zp_modifiziert
            && zp_mod < unchecked.zp_start
        {
            return Err(de::Error::custom("zp_start <= zp_modifiziert did not hold"));
        }
        // constraint: zp_start cannot be in the future
        if unchecked.zp_start > chrono::Utc::now() {
            return Err(de::Error::custom(
                "Stations cannot happen in the past (zp_start <= now). For more Context see <https://chat.pazufa.de/parlamentszusammenfasser/pl/45jic8o6rjnk8djc1auy7s638o> or <https://codeberg.org/PaZuFa/parlamentszusammenfasser/issues/66>",
            ));
        }
        Ok(unchecked)
    }
}
impl Serialize for Station {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        Station::serialize(self, serializer)
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
                KeyedSet::validate(&hashlist).map_err(|e| {
                    serde::de::Error::custom(format!("At most one hash per strategy: {e}"))
                })?;
                // DokumentHash.value was already checked by its field rule (HexDigest)
                if hashlist.iter().all(|x| {
                    x.strategy != HashStrategy::Sha256Text
                        || matches!(x.mime, Mime::TextHtml | Mime::TextPlain)
                }) {
                    return Ok(Self::Future(hashlist));
                }
                return Err(serde::de::Error::custom(
                    "Some Hash failed the mime/hash sanity check: sha256+text requires mime text/html or text/plain",
                ));
            }
            Err(e) => hl_parsing_err = Some(e.to_string()),
        }
        match String::deserialize(value.clone()) {
            Ok(hash) => {
                HexDigest::validate(&hash)
                    .map_err(|e| serde::de::Error::custom(format!("Legacy Hash: {e}")))?;
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
            Text::validate(&summ).map_err(serde::de::Error::custom)?;
            return Ok(Self::Legacy(summ));
        }

        match <Vec<Zusammenfassungstupel>>::deserialize(value.clone()) {
            Ok(tupellist) => {
                KeyedSet::validate(&tupellist).map_err(|e| {
                    serde::de::Error::custom(format!("At most one summary per typ: {e}"))
                })?;
                Ok(Self::Future(tupellist))
            }
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
impl<'de> Deserialize<'de> for Sitzung {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let unchecked = Sitzung::deserialize(deserializer)?;
        // custom validation: Dokumente should be objects at deserialization time
        let api_ids: Vec<_> = unchecked
            .dokumente
            .iter()
            .enumerate()
            .filter_map(|(index, thing)| match thing {
                DokumentOrApiId::ApiId(i) => Some((index, i)),
                DokumentOrApiId::Dokument(_) => None,
            })
            .collect();
        if !api_ids.is_empty() {
            return Err(de::Error::custom(format!(
                "Documents Cannot be api ids at upload time, only full objects. The following entries of sitzung.dokumente were api ids(index, api_id): {api_ids:?}"
            )));
        }

        // link format is checked by its field rule (Link)
        Ok(unchecked)
    }
}
impl Serialize for Sitzung {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        Sitzung::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for Top {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let unchecked = Self::deserialize(deserializer)?;
        // custom validation: Dokumente should be objects at deserialization time
        let api_ids: Vec<_> = unchecked
            .dokumente
            .iter()
            .enumerate()
            .filter_map(|(index, thing)| match thing {
                DokumentOrApiId::ApiId(i) => Some((index, i)),
                DokumentOrApiId::Dokument(_) => None,
            })
            .collect();
        if api_ids.is_empty() {
            Ok(unchecked)
        } else {
            Err(de::Error::custom(format!(
                "Documents Cannot be api ids at upload time, only full objects. The following entries of top.dokumente were api ids(index, api_id): {api_ids:?}"
            )))
        }
    }
}
impl Serialize for Top {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for Lobbyregistereintrag {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let unchecked = Self::deserialize(deserializer)?;
        // constraint link(format=uri)
        if let Err(e) = axum::http::Uri::from_str(&unchecked.link) {
            Err(de::Error::custom(e.to_string()))
        } else {
            Ok(unchecked)
        }
    }
}
impl Serialize for Lobbyregistereintrag {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for KalGetQueryParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let unchecked = Self::deserialize(deserializer)?;
        // constraint y(>1944)
        if let Some(y) = unchecked.y
            && y < 1945
        {
            return Err(de::Error::custom("Error: Year must be >= 1945".to_owned()));
        }
        // constraint dom(<31)
        if let Some(m) = unchecked.m
            && !(1..=12).contains(&m)
        {
            return Err(de::Error::custom(
                "Error: Month must be between 1 and 12".to_owned(),
            ));
        }
        // constraint m(1<=m<=12)
        if let Some(dom) = unchecked.dom
            && !(1..=31).contains(&dom)
        {
            return Err(de::Error::custom(
                "Error: DoM must be in valid value range".to_owned(),
            ));
        }
        Ok(unchecked)
    }
}
impl Serialize for KalGetQueryParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        Self::serialize(self, serializer)
    }
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
