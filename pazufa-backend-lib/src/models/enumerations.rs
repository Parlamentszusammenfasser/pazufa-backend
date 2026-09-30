use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use utoipa::ToSchema;

macro_rules! string_enum_convenience {
    ($name:ident) => {
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let s = serde_json::to_string(self).map_err(|_| std::fmt::Error)?;
                write!(f, "{}", &s[1..s.len().saturating_sub(1)])
            }
        }

        impl std::str::FromStr for $name {
            type Err = String;

            fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
                serde_json::from_str(&format!("\"{s}\"")).map_err(|x| x.to_string())
            }
        }
    };
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, ToSchema)]
#[sqlx(type_name = "\"APIScope\"")]
pub enum APIScope {
    #[sqlx(rename = "keyadder")]
    #[serde(rename = "keyadder")]
    KeyAdder,
    #[sqlx(rename = "admin")]
    #[serde(rename = "admin")]
    Admin,
    #[sqlx(rename = "collector")]
    #[serde(rename = "collector")]
    Collector,
}
/// Type of a Document
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
pub enum Doktyp {
    /// Pre-lawtext drafts. Mostly Eckpunktepapiere
    #[serde(rename = "eckpunktepapier")]
    Eckpunktepapier,
    /// Preparliamentary draft legislation, to be used for Referentenentwürfe and similar documents
    #[serde(rename = "preparl-entwurf")]
    PreparlEntwurf,
    /// A law draft within the parliament.
    /// Used for all kinds of drafts within the parliamentary process
    #[serde(rename = "entwurf")]
    Entwurf,
    /// Antrag
    #[serde(rename = "antrag")]
    Antrag,
    /// Questioning
    #[serde(rename = "anfrage")]
    Anfrage,
    /// Answer to a Questioning
    #[serde(rename = "antwort")]
    Antwort,
    /// Some memo in the parliament
    #[serde(rename = "mitteilung")]
    Mitteilung,
    /// Recommendation of a committee
    #[serde(rename = "beschlussempf")]
    Beschlussempf,
    /// Statement of a person or organisation
    #[serde(rename = "stellungnahme")]
    Stellungnahme,
    /// Evaluation
    #[serde(rename = "gutachten")]
    Gutachten,
    /// Protocol of the talking points
    #[serde(rename = "redeprotokoll")]
    Redeprotokoll,
    /// Protocol of what was decided
    #[serde(rename = "beschlussprotokoll")]
    Beschlussprotokoll,
    /// Protocol of the meeting content
    #[serde(rename = "inhaltsprotokoll")]
    Inhaltsprotokoll,
    /// Agenda Items
    #[serde(rename = "tops")]
    Tops,
    /// Changing of Agenda Items
    #[serde(rename = "tops-aend")]
    TopsAend,
    /// Amendments to Agenda Items
    #[serde(rename = "tops-ergz")]
    TopsErgz,
    /// Finalized and released Law text
    #[serde(rename = "gesetz")]
    Gesetz,
    /// Other. Catch-all
    #[serde(rename = "sonstig")]
    Sonstig,
}
string_enum_convenience!(Doktyp);

/// Enumeration of parliaments or similar bodies in germany
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

string_enum_convenience!(Parlament);

/// The legislative Track we are on. Together with a parliament, this tells us about the possible stations that can occurr within
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
pub enum Vorgangstyp {
    #[serde(rename = "bu-einspruch-inibreg")]
    BuEinspruchInibreg,
    #[serde(rename = "bu-einspruch-inibreg-haushalt")]
    BuEinspruchInibregHaushalt,
    #[serde(rename = "bu-zustimmung-inibreg")]
    BuZustimmungInibreg,
    #[serde(rename = "bu-zustimmung-inibreg-haushalt")]
    BuZustimmungInibregHaushalt,
    #[serde(rename = "bu-einspruch-inibreg-intvertrag")]
    BuEinspruchInibregIntvertrag,
    #[serde(rename = "bu-zustimmung-inibreg-intvertrag")]
    BuZustimmungInibregIntvertrag,
    #[serde(rename = "bu-einspruch-inisonst")]
    BuEinspruchInisonst,

    #[serde(rename = "bu-einspruch-inisonst-intvertrag")]
    BuEinspruchInisonstIntvertrag,
    #[serde(rename = "bu-zustimmung-inisonst")]
    BuZustimmungInisonst,
    #[serde(rename = "bu-zustimmung-inisonst-intvertrag")]
    BuZustimmungInisonstIntvertrag,
    #[serde(rename = "bu-antrag-bweinsatz")]
    BuAntragBweinsatz,
    #[serde(rename = "gg-land-parl")]
    GgLandParl,
    #[serde(rename = "gg-land-parl-haushalt")]
    GgLandParlHaushalt,
    #[serde(rename = "gg-land-volk")]
    GgLandVolk,
    #[serde(rename = "bw-einsatz")]
    BwEinsatz,
    #[serde(rename = "sonstig")]
    Sonstig,
}

string_enum_convenience!(Vorgangstyp);

/// Substantive legislative steps, that a Vorgang can pass on its way to a law or rejection or similar endings.
/// The categorisation pre/postparl is to a degree arbitrary, these are just identifiers.
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
    #[serde(rename = "preparl-formvs")]
    PreparlFormvs,
    #[serde(rename = "parl-initiativ")]
    ParlInitiativ,
    #[serde(rename = "parl-antragsst")]
    ParlAntragsst,
    #[serde(rename = "parl-anhoerung")]
    ParlAnhoerung,
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
    #[serde(rename = "parl-vermittas")]
    ParlVermittas,
    #[serde(rename = "postparl-vgstp")]
    PostparlVgstp,
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
string_enum_convenience!(Stationstyp);

/// Type of an identifier of a Vorgang. Can contain whatever a parliament uses to identify any complete process.
/// Currently in existence: vorgnr, api-id, initdrucks, sonstig.
/// If possible, please do not invent any type like `inids`, and use the ones in existence
#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize, ToSchema)]
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

/// Enumeration of values
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

string_enum_convenience!(EnumerationNames);

/// Ressort, which the Vorgang is associated with. Usually the name of a ministry
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
pub enum Ressort {
    #[serde(rename = "Arbeit")]
    Arbeit,
    #[serde(rename = "Bildung")]
    Bildung,
    #[serde(rename = "Digitalisierung")]
    Digitalisierung,
    #[serde(rename = "Energie")]
    Energie,
    #[serde(rename = "Ernährung")]
    Ernährung,
    #[serde(rename = "Europa")]
    Europa,
    #[serde(rename = "Familie/Senioren")]
    FamilieSenioren,
    #[serde(rename = "Finanzen")]
    Finanzen,
    #[serde(rename = "Forschung")]
    Forschung,
    #[serde(rename = "Forsten")]
    Forsten,
    #[serde(rename = "Frauen/Gleichstellung")]
    FrauenGleichstellung,
    #[serde(rename = "Gesundheit/Pflege/Prävention")]
    GesundheitPflegePrävention,
    #[serde(rename = "Heimat")]
    Heimat,
    #[serde(rename = "Inneres")]
    Inneres,
    #[serde(rename = "Integration/Migration")]
    IntegrationMigration,
    #[serde(rename = "Jugend")]
    Jugend,
    #[serde(rename = "Justiz")]
    Justiz,
    #[serde(rename = "Kinder")]
    Kinder,
    #[serde(rename = "Klimaschutz")]
    Klimaschutz,
    #[serde(rename = "Kommunales")]
    Kommunales,
    #[serde(rename = "Kunst/Kultur")]
    KunstKultur,
    #[serde(rename = "Landes-/Stadtentwicklung")]
    LandesStadtentwicklung,
    #[serde(rename = "Ländlicher Raum")]
    LändlicherRaum,
    #[serde(rename = "Landwirtschaft")]
    Landwirtschaft,
    #[serde(rename = "Soziales")]
    Soziales,
    #[serde(rename = "Sport")]
    Sport,
    #[serde(rename = "Tourismus")]
    Tourismus,
    #[serde(rename = "Umwelt")]
    Umwelt,
    #[serde(rename = "Verbraucherschutz")]
    Verbraucherschutz,
    #[serde(rename = "Verkehr/Infrastruktur")]
    VerkehrInfrastruktur,
    #[serde(rename = "Wirtschaft")]
    Wirtschaft,
    #[serde(rename = "Wissenschaft")]
    Wissenschaft,
    #[serde(rename = "Wohnen/Bau")]
    WohnenBau,
}
string_enum_convenience!(Ressort);

/// Sachgebiet / Themenbereich inspired by the  Parlamentsspiegel.
/// Within the Parlamentsspiegel search this corresponds to the parameter `fqVSys`
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize_repr, Deserialize_repr, ToSchema,
)]
#[repr(u16)]
pub enum Sachgebiet {
    StaatundPolitik = 1000,
    Staatsaufbau = 1010,
    Menschenrechte = 1030,
    Nation = 1050,
    Ideologien = 1060,
    PolitischeKräfte = 1070,
    Wahlen = 1080,
    Parlament = 1100,
    Abgeordnete = 1110,
    ÖffentlicheVerwaltung = 1200,
    Bundesregierung = 1210,
    Landesregierung = 1220,
    KommunaleAngelegenheiten = 1230,
    ÖffentlicherDienst = 1240,
    InnereSicherheit = 1300,
    Polizei = 1310,
    VerfassungsschutzSpionage = 1320,
    Ordnungsrecht = 1330,
    KatastrophenundZivilschutz = 1340,
    Rettungswesen = 1350,
    Verteidigung = 1400,
    Wehrdienst = 1410,
    Rüstung = 1420,
    AbrüstungKonversion = 1430,
    Außenpolitik = 1500,
    InternationaleBeziehungen = 1510,
    InternationaleOrganisationen = 1520,
    Entwicklungszusammenarbeit = 1530,
    Europapolitik = 1540,
    EuropäischeUnion = 1600,
    OrganederEU = 1610,
    ProgrammederEU = 1620,
    Wirtschaft = 2000,
    GewerblicheWirtschaftIndustrie = 2010,
    Handel = 2020,
    Dienstleistungen = 2030,
    Versicherungen = 2040,
    MittelständischeWirtschaft = 2050,
    Außenwirtschaft = 2060,
    Verbraucher = 2070,
    PreisundKartellrecht = 2080,
    Gewerbeaufsicht = 2090,
    Energie = 2100,
    FossileEnergien = 2110,
    Kernenergie = 2120,
    ErneuerbareEnergien = 2130,
    BergbauBodenschätze = 2200,
    Technologie = 2300,
    ArbeitundBeschäftigung = 2400,
    Arbeitsmarkt = 2410,
    Berufsausbildung = 2420,
    Arbeitsentgelt = 2430,
    Mitbestimmung = 2440,
    Arbeitsbedingungen = 2450,
    Standardisierung = 2500,
    Normung = 2510,
    EichundMesswesen = 2520,
    VermessungsundKatasterwesen = 2530,
    Verkehr = 2600,
    ÖffentlicherPersonenverkehr = 2610,
    Güterverkehr = 2620,
    Straßenverkehr = 2630,
    Schienenverkehr = 2640,
    Luftverkehr = 2650,
    Schifffahrt = 2660,
    Raumfahrt = 2700,
    Bauwesen = 2800,
    Verkehrswegebau = 2810,
    Städtebau = 2820,
    Wohnungswesen = 2830,
    Wasserbau = 2840,
    Recht = 3100,
    Strafrecht = 3110,
    Zivilrecht = 3120,
    ÖffentlichesRecht = 3130,
    Urheberschutz = 3140,
    Verfassungsgerichtsbarkeit = 3200,
    Justiz = 3300,
    GerichteundStaatsanwaltschaften = 3310,
    Justizverwaltung = 3320,
    Justizvollzug = 3330,
    JuristischeBerufe = 3400,
    Bildung = 4100,
    Schulen = 4200,
    Lehrer = 4210,
    AllgemeinbildendeSchulen = 4220,
    BerufsbildendeSchulen = 4230,
    Sonderpädagogik = 4240,
    Privatschulen = 4250,
    FrühkindlicheBildung = 4260,
    Hochschulwesen = 4300,
    Universitäten = 4310,
    KunstundMusikhochschulen = 4320,
    HochschulenfürangewandteWissenschaften = 4330,
    WissenschaftForschung = 4400,
    Erwachsenenbildung = 4500,
    GesellschaftBevölkerung = 5000,
    Lebensgemeinschaften = 5010,
    SexuelleIdentität = 5020,
    KinderJugendliche = 5030,
    FrauenMänner = 5040,
    MenschenmitBehinderungen = 5050,
    AlteMenschen = 5060,
    AusländerMigranten = 5070,
    SonstigegesellschaftlicheGruppen = 5080,
    Soziales = 5100,
    Sozialversicherung = 5110,
    Sozialleistungen = 5120,
    SozialeEinrichtungen = 5130,
    Versorgung = 5140,
    Pflege = 5150,
    Gesundheit = 5200,
    Gesundheitsschutz = 5210,
    Gesundheitseinrichtungen = 5220,
    MedizinischeBerufe = 5230,
    Arzneimittel = 5240,
    Rauschmittel = 5250,
    Psychiatrie = 5260,
    Tod = 5270,
    Umwelt = 6100,
    Natur = 6110,
    TierTierschutzTierhaltung = 6120,
    Boden = 6130,
    Wasser = 6140,
    Klima = 6150,
    SchadstoffeImmissionenundEmissionen = 6160,
    Abfall = 6200,
    Abwasser = 6300,
    Raumordnung = 6400,
    LändlicherRaum = 6410,
    Landwirtschaft = 6500,
    LandwirtschaftlicheBetriebe = 6510,
    Agrarmarkt = 6520,
    LandwirtschaftlicheBerufe = 6530,
    WaldForsten = 6600,
    JagdFischerei = 6700,
    Ernährung = 6800,
    Tierkrankheiten = 6900,
    KunstKultur = 7100,
    DenkmalschutzDenkmalpflege = 7200,
    Religionsgemeinschaften = 7300,
    Freizeit = 7400,
    MessenAusstellungen = 7500,
    Sport = 7600,
    InformationsgesellschaftMedien = 7700,
    Printmedien = 7710,
    RundfunkFernsehen = 7720,
    FilmVideo = 7730,
    InformationsundKommunikationstechnologien = 7740,
    Datenschutz = 7750,
    Statistik = 7800,
    Abgaben = 8100,
    Finanzverwaltung = 8200,
    ÖffentlicherHaushalt = 8300,
    ÖffentlicheSchulden = 8310,
    ÖffentlichesVermögen = 8320,
    Haushaltskontrolle = 8330,
    Finanzausgleich = 8340,
    ÖffentlicheVergabe = 8350,
    Finanzmarkt = 8400,
    Vermögen = 8600,
    Glücksspiel = 8700,
    Stiftung = 8800,
    Unbekannt = 9900,
    OhneSystematik = 9999,
}
impl From<i32> for Sachgebiet {
    fn from(value: i32) -> Self {
        serde_json::from_value(serde_json::Value::Number(
            serde_json::Number::from_i128(i128::from(value)).expect("number"),
        ))
        .expect("Expected a value that actually deserializes to Sachgebiet")
    }
}
impl From<Sachgebiet> for i32 {
    fn from(value: Sachgebiet) -> Self {
        match serde_json::to_value(value).expect("Expected the value to fit into number") {
            serde_json::Value::Number(n) => {
                i32::try_from(n.as_i64().expect("this number is small enough"))
                    .expect("Number is small enough")
            }
            _ => {
                unreachable!()
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
pub enum Mime {
    #[serde(rename = "application/json")]
    ApplicationJson,
    #[serde(rename = "application/pdf")]
    ApplicationPdf,
    #[serde(rename = "text/plain")]
    TextPlain,
    #[serde(rename = "text/html")]
    TextHtml,
}
string_enum_convenience!(Mime);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema, Default,
)]
pub enum HashStrategy {
    #[default]
    #[serde(rename = "sha256+bytes")]
    Sha256Bytes,
    #[serde(rename = "sha256+text")]
    Sha256Text,
    #[serde(rename = "sha1+bytes")]
    Sha1Bytes,
}
string_enum_convenience!(HashStrategy);

/// Format of return of the documents. Defaults to pazufa
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, Default)]
pub enum DokumentFormat {
    #[serde(rename = "ftm")]
    FTM,
    #[serde(rename = "pazufa")]
    #[default]
    Pazufa,
}

#[cfg(test)]
mod test {
    use std::str::FromStr;
    use std::string::ToString;

    #[test]
    fn string_enum_convenience_tostring() {
        use super::Stationstyp::*;
        let thing = [PreparlEckpup, ParlAntragsst];
        let answr = ["preparl-eckpup", "parl-antragsst"];

        for (t, a) in thing.iter().zip(answr) {
            assert_eq!(t.to_string(), a);
        }
    }
    #[test]
    fn string_enum_convenience_from_str() {
        use super::Stationstyp::*;
        let thing = [PreparlEckpup, ParlAntragsst];
        let answr = ["preparl-eckpup", "parl-antragsst"];

        for (t, a) in thing.iter().zip(answr) {
            assert_eq!(t, &super::Stationstyp::from_str(a).expect(""));
        }
    }
}
