use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Typ eines Dokuments. Beschreibt explizit nicht den Kontext, sondern nur den Typ des Inhalts. (z.B. 'tops' kann sowohl für Plenarsitzungen als auch für Ausschusssitzungen relevant sein)
/// Enumeration of values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
pub enum Doktyp {
    #[serde(rename = "preparl-entwurf")]
    PreparlEntwurf,
    #[serde(rename = "entwurf")]
    Entwurf,
    #[serde(rename = "antrag")]
    Antrag,
    #[serde(rename = "anfrage")]
    Anfrage,
    #[serde(rename = "antwort")]
    Antwort,
    #[serde(rename = "mitteilung")]
    Mitteilung,
    #[serde(rename = "beschlussempf")]
    Beschlussempf,
    #[serde(rename = "stellungnahme")]
    Stellungnahme,
    #[serde(rename = "gutachten")]
    Gutachten,
    #[serde(rename = "redeprotokoll")]
    Redeprotokoll,
    #[serde(rename = "tops")]
    Tops,
    #[serde(rename = "tops-aend")]
    TopsAend,
    #[serde(rename = "tops-ergz")]
    TopsErgz,
    #[serde(rename = "sonstig")]
    Sonstig,
}

impl std::fmt::Display for Doktyp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Doktyp::PreparlEntwurf => write!(f, "preparl-entwurf"),
            Doktyp::Entwurf => write!(f, "entwurf"),
            Doktyp::Antrag => write!(f, "antrag"),
            Doktyp::Anfrage => write!(f, "anfrage"),
            Doktyp::Antwort => write!(f, "antwort"),
            Doktyp::Mitteilung => write!(f, "mitteilung"),
            Doktyp::Beschlussempf => write!(f, "beschlussempf"),
            Doktyp::Stellungnahme => write!(f, "stellungnahme"),
            Doktyp::Gutachten => write!(f, "gutachten"),
            Doktyp::Redeprotokoll => write!(f, "redeprotokoll"),
            Doktyp::Tops => write!(f, "tops"),
            Doktyp::TopsAend => write!(f, "tops-aend"),
            Doktyp::TopsErgz => write!(f, "tops-ergz"),
            Doktyp::Sonstig => write!(f, "sonstig"),
        }
    }
}

impl std::str::FromStr for Doktyp {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "preparl-entwurf" => std::result::Result::Ok(Doktyp::PreparlEntwurf),
            "entwurf" => std::result::Result::Ok(Doktyp::Entwurf),
            "antrag" => std::result::Result::Ok(Doktyp::Antrag),
            "anfrage" => std::result::Result::Ok(Doktyp::Anfrage),
            "antwort" => std::result::Result::Ok(Doktyp::Antwort),
            "mitteilung" => std::result::Result::Ok(Doktyp::Mitteilung),
            "beschlussempf" => std::result::Result::Ok(Doktyp::Beschlussempf),
            "stellungnahme" => std::result::Result::Ok(Doktyp::Stellungnahme),
            "gutachten" => std::result::Result::Ok(Doktyp::Gutachten),
            "redeprotokoll" => std::result::Result::Ok(Doktyp::Redeprotokoll),
            "tops" => std::result::Result::Ok(Doktyp::Tops),
            "tops-aend" => std::result::Result::Ok(Doktyp::TopsAend),
            "tops-ergz" => std::result::Result::Ok(Doktyp::TopsErgz),
            "sonstig" => std::result::Result::Ok(Doktyp::Sonstig),
            _ => std::result::Result::Err(format!("Value not valid: {s}")),
        }
    }
}

/// Enumeration der Parlamentsähnlichen Entscheidungscorpi in Deutschland
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
pub enum Parlament {
    #[serde(rename = "BT")]
    Bt,
    #[serde(rename = "BR")]
    Br,
    #[serde(rename = "BV")]
    Bv,
    #[serde(rename = "EK")]
    Ek,
    #[serde(rename = "BB")]
    Bb,
    #[serde(rename = "BY")]
    By,
    #[serde(rename = "BE")]
    Be,
    #[serde(rename = "HB")]
    Hb,
    #[serde(rename = "HH")]
    Hh,
    #[serde(rename = "HE")]
    He,
    #[serde(rename = "MV")]
    Mv,
    #[serde(rename = "NI")]
    Ni,
    #[serde(rename = "NW")]
    Nw,
    #[serde(rename = "RP")]
    Rp,
    #[serde(rename = "SL")]
    Sl,
    #[serde(rename = "SN")]
    Sn,
    #[serde(rename = "TH")]
    Th,
    #[serde(rename = "SH")]
    Sh,
    #[serde(rename = "BW")]
    Bw,
    #[serde(rename = "ST")]
    St,
}

impl std::fmt::Display for Parlament {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Parlament::Bt => write!(f, "BT"),
            Parlament::Br => write!(f, "BR"),
            Parlament::Bv => write!(f, "BV"),
            Parlament::Ek => write!(f, "EK"),
            Parlament::Bb => write!(f, "BB"),
            Parlament::By => write!(f, "BY"),
            Parlament::Be => write!(f, "BE"),
            Parlament::Hb => write!(f, "HB"),
            Parlament::Hh => write!(f, "HH"),
            Parlament::He => write!(f, "HE"),
            Parlament::Mv => write!(f, "MV"),
            Parlament::Ni => write!(f, "NI"),
            Parlament::Nw => write!(f, "NW"),
            Parlament::Rp => write!(f, "RP"),
            Parlament::Sl => write!(f, "SL"),
            Parlament::Sn => write!(f, "SN"),
            Parlament::Th => write!(f, "TH"),
            Parlament::Sh => write!(f, "SH"),
            Parlament::Bw => write!(f, "BW"),
            Parlament::St => write!(f, "ST"),
        }
    }
}

impl std::str::FromStr for Parlament {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "BT" => std::result::Result::Ok(Parlament::Bt),
            "BR" => std::result::Result::Ok(Parlament::Br),
            "BV" => std::result::Result::Ok(Parlament::Bv),
            "EK" => std::result::Result::Ok(Parlament::Ek),
            "BB" => std::result::Result::Ok(Parlament::Bb),
            "BY" => std::result::Result::Ok(Parlament::By),
            "BE" => std::result::Result::Ok(Parlament::Be),
            "HB" => std::result::Result::Ok(Parlament::Hb),
            "HH" => std::result::Result::Ok(Parlament::Hh),
            "HE" => std::result::Result::Ok(Parlament::He),
            "MV" => std::result::Result::Ok(Parlament::Mv),
            "NI" => std::result::Result::Ok(Parlament::Ni),
            "NW" => std::result::Result::Ok(Parlament::Nw),
            "RP" => std::result::Result::Ok(Parlament::Rp),
            "SL" => std::result::Result::Ok(Parlament::Sl),
            "SN" => std::result::Result::Ok(Parlament::Sn),
            "TH" => std::result::Result::Ok(Parlament::Th),
            "SH" => std::result::Result::Ok(Parlament::Sh),
            "BW" => std::result::Result::Ok(Parlament::Bw),
            "ST" => std::result::Result::Ok(Parlament::St),
            _ => std::result::Result::Err(format!("Value not valid: {s}")),
        }
    }
}
/// Der Gesetzgebungstrack auf dem wir uns befinden. Zum Beispiel: gesetzgebung - Einspruchsgesetz. Legt fest, welche Stationen im Vorgang möglich sind zusammen mit den Parlamenten in den Stationen
/// Enumeration of values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
pub enum Vorgangstyp {
    #[serde(rename = "gg-einspruch")]
    GgEinspruch,
    #[serde(rename = "gg-zustimmung")]
    GgZustimmung,
    #[serde(rename = "gg-land-parl")]
    GgLandParl,
    #[serde(rename = "gg-land-volk")]
    GgLandVolk,
    #[serde(rename = "bw-einsatz")]
    BwEinsatz,
    #[serde(rename = "sonstig")]
    Sonstig,
}

impl std::fmt::Display for Vorgangstyp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Vorgangstyp::GgEinspruch => write!(f, "gg-einspruch"),
            Vorgangstyp::GgZustimmung => write!(f, "gg-zustimmung"),
            Vorgangstyp::GgLandParl => write!(f, "gg-land-parl"),
            Vorgangstyp::GgLandVolk => write!(f, "gg-land-volk"),
            Vorgangstyp::BwEinsatz => write!(f, "bw-einsatz"),
            Vorgangstyp::Sonstig => write!(f, "sonstig"),
        }
    }
}

impl std::str::FromStr for Vorgangstyp {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "gg-einspruch" => std::result::Result::Ok(Vorgangstyp::GgEinspruch),
            "gg-zustimmung" => std::result::Result::Ok(Vorgangstyp::GgZustimmung),
            "gg-land-parl" => std::result::Result::Ok(Vorgangstyp::GgLandParl),
            "gg-land-volk" => std::result::Result::Ok(Vorgangstyp::GgLandVolk),
            "bw-einsatz" => std::result::Result::Ok(Vorgangstyp::BwEinsatz),
            "sonstig" => std::result::Result::Ok(Vorgangstyp::Sonstig),
            _ => std::result::Result::Err(format!("Value not valid: {s}")),
        }
    }
}

/// Stationen, die ein Gesetz o.Ä. Vorgang in verschiedenen Ausformungen durchlaufen kann. Im Prinzip ist die Festlegung auf Prä / Postparlamentarisch arbiträr.
/// Enumeration of values.
/// Since this enum's variants do not hold data, we can easily define them as `#[repr(C)]`
/// which helps with FFI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
pub enum Stationstyp {
    #[serde(rename = "preparl-regent")]
    PreparlRegent,
    #[serde(rename = "preparl-eckpup")]
    PreparlEckpup,
    #[serde(rename = "preparl-regbsl")]
    PreparlRegbsl,
    #[serde(rename = "preparl-vbegde")]
    PreparlVbegde,
    #[serde(rename = "parl-initiativ")]
    ParlInitiativ,
    #[serde(rename = "parl-ausschber")]
    ParlAusschber,
    #[serde(rename = "parl-vollvlsgn")]
    ParlVollvlsgn,
    #[serde(rename = "parl-akzeptanz")]
    ParlAkzeptanz,
    #[serde(rename = "parl-ablehnung")]
    ParlAblehnung,
    #[serde(rename = "parl-zurueckgz")]
    ParlZurueckgz,
    #[serde(rename = "parl-ggentwurf")]
    ParlGgentwurf,
    #[serde(rename = "postparl-vesja")]
    PostparlVesja,
    #[serde(rename = "postparl-vesne")]
    PostparlVesne,
    #[serde(rename = "postparl-gsblt")]
    PostparlGsblt,
    #[serde(rename = "postparl-kraft")]
    PostparlKraft,
    #[serde(rename = "sonstig")]
    Sonstig,
}

impl std::fmt::Display for Stationstyp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Stationstyp::PreparlRegent => write!(f, "preparl-regent"),
            Stationstyp::PreparlEckpup => write!(f, "preparl-eckpup"),
            Stationstyp::PreparlRegbsl => write!(f, "preparl-regbsl"),
            Stationstyp::PreparlVbegde => write!(f, "preparl-vbegde"),
            Stationstyp::ParlInitiativ => write!(f, "parl-initiativ"),
            Stationstyp::ParlAusschber => write!(f, "parl-ausschber"),
            Stationstyp::ParlVollvlsgn => write!(f, "parl-vollvlsgn"),
            Stationstyp::ParlAkzeptanz => write!(f, "parl-akzeptanz"),
            Stationstyp::ParlAblehnung => write!(f, "parl-ablehnung"),
            Stationstyp::ParlZurueckgz => write!(f, "parl-zurueckgz"),
            Stationstyp::ParlGgentwurf => write!(f, "parl-ggentwurf"),
            Stationstyp::PostparlVesja => write!(f, "postparl-vesja"),
            Stationstyp::PostparlVesne => write!(f, "postparl-vesne"),
            Stationstyp::PostparlGsblt => write!(f, "postparl-gsblt"),
            Stationstyp::PostparlKraft => write!(f, "postparl-kraft"),
            Stationstyp::Sonstig => write!(f, "sonstig"),
        }
    }
}

impl std::str::FromStr for Stationstyp {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "preparl-regent" => std::result::Result::Ok(Stationstyp::PreparlRegent),
            "preparl-eckpup" => std::result::Result::Ok(Stationstyp::PreparlEckpup),
            "preparl-regbsl" => std::result::Result::Ok(Stationstyp::PreparlRegbsl),
            "preparl-vbegde" => std::result::Result::Ok(Stationstyp::PreparlVbegde),
            "parl-initiativ" => std::result::Result::Ok(Stationstyp::ParlInitiativ),
            "parl-ausschber" => std::result::Result::Ok(Stationstyp::ParlAusschber),
            "parl-vollvlsgn" => std::result::Result::Ok(Stationstyp::ParlVollvlsgn),
            "parl-akzeptanz" => std::result::Result::Ok(Stationstyp::ParlAkzeptanz),
            "parl-ablehnung" => std::result::Result::Ok(Stationstyp::ParlAblehnung),
            "parl-zurueckgz" => std::result::Result::Ok(Stationstyp::ParlZurueckgz),
            "parl-ggentwurf" => std::result::Result::Ok(Stationstyp::ParlGgentwurf),
            "postparl-vesja" => std::result::Result::Ok(Stationstyp::PostparlVesja),
            "postparl-vesne" => std::result::Result::Ok(Stationstyp::PostparlVesne),
            "postparl-gsblt" => std::result::Result::Ok(Stationstyp::PostparlGsblt),
            "postparl-kraft" => std::result::Result::Ok(Stationstyp::PostparlKraft),
            "sonstig" => std::result::Result::Ok(Stationstyp::Sonstig),
            _ => std::result::Result::Err(format!("Value not valid: {s}")),
        }
    }
}
/// Typ von Identifikatoren für einen gesamten Vorgang. Offen für was auch immer ein Parlament benutzt um einen Vorgang zu identifizieren. Aktuell in der Datenbank: initdrucks, vorgnr, api-id, sonstig bitte keine neue Abkürzung für denselben Typ erfinden
#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct VgIdentTyp(pub String);
impl std::convert::From<String> for VgIdentTyp {
    fn from(x: String) -> Self {
        VgIdentTyp(x)
    }
}

impl std::fmt::Display for VgIdentTyp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for VgIdentTyp {
    type Err = std::string::ParseError;
    fn from_str(x: &str) -> std::result::Result<Self, Self::Err> {
        std::result::Result::Ok(VgIdentTyp(x.to_string()))
    }
}

impl std::convert::From<VgIdentTyp> for String {
    fn from(x: VgIdentTyp) -> Self {
        x.0
    }
}

impl std::ops::Deref for VgIdentTyp {
    type Target = String;
    fn deref(&self) -> &String {
        &self.0
    }
}

impl std::ops::DerefMut for VgIdentTyp {
    fn deref_mut(&mut self) -> &mut String {
        &mut self.0
    }
}

/// Enumeration of values.
/// Since this enum's variants do not hold data, we can easily define them as `#[repr(C)]`
/// which helps with FFI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
pub enum EnumerationNames {
    #[serde(rename = "schlagworte")]
    Schlagworte,
    #[serde(rename = "stationstypen")]
    Stationstypen,
    #[serde(rename = "vorgangstypen")]
    Vorgangstypen,
    #[serde(rename = "parlamente")]
    Parlamente,
    #[serde(rename = "vgidtypen")]
    Vgidtypen,
    #[serde(rename = "dokumententypen")]
    Dokumententypen,
}
impl std::fmt::Display for EnumerationNames {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            EnumerationNames::Schlagworte => write!(f, "schlagworte"),
            EnumerationNames::Stationstypen => write!(f, "stationstypen"),
            EnumerationNames::Vorgangstypen => write!(f, "vorgangstypen"),
            EnumerationNames::Parlamente => write!(f, "parlamente"),
            EnumerationNames::Vgidtypen => write!(f, "vgidtypen"),
            EnumerationNames::Dokumententypen => write!(f, "dokumententypen"),
        }
    }
}

impl std::str::FromStr for EnumerationNames {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "schlagworte" => std::result::Result::Ok(EnumerationNames::Schlagworte),
            "stationstypen" => std::result::Result::Ok(EnumerationNames::Stationstypen),
            "vorgangstypen" => std::result::Result::Ok(EnumerationNames::Vorgangstypen),
            "parlamente" => std::result::Result::Ok(EnumerationNames::Parlamente),
            "vgidtypen" => std::result::Result::Ok(EnumerationNames::Vgidtypen),
            "dokumententypen" => std::result::Result::Ok(EnumerationNames::Dokumententypen),
            _ => std::result::Result::Err(format!("Value not valid: {s}")),
        }
    }
}
