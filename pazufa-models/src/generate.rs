use crate::{
    self as models,
    dokument::{HashWrapper, ZusammenfassungWrapper},
};

pub(crate) mod random {
    use arbitrary::{Arbitrary, Unstructured};
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    pub(crate) fn with_rng<T>(rng: &mut StdRng) -> T
    where
        for<'a> T: Arbitrary<'a>,
    {
        let (sz_min, sz_max) = T::size_hint(0);

        let mut random_data = vec![0u8; 1 << 24];
        rng.fill_bytes(&mut random_data);
        let unstructured = Unstructured::new(&random_data);
        T::arbitrary_take_rest(unstructured).unwrap_or_else(|e| {
                panic!(
                    "Model Generation failed with this size hint: {sz_min} < x < {sz_max:?} and this error: {e}"
                )
            })
    }
    pub(crate) fn seeded<T>(seed: u64) -> T
    where
        for<'a> T: Arbitrary<'a>,
    {
        let mut rng = StdRng::seed_from_u64(seed);
        with_rng(&mut rng)
    }
}

pub(crate) fn default_station() -> models::Station {
    models::Station {
        api_id: Some(uuid::uuid!("b18bde64-c0ff-eeee-ff0c-deadbeefeeee")),
        typ: models::Stationstyp::ParlInitiativ,
        link: Some("https://an.example.com/leckmichfett".to_string()),
        gremium_federf: None,
        titel: Some("rattlesnakes!".to_string()),
        zp_start: chrono::DateTime::parse_from_rfc3339("1950-01-01T22:01:02+00:00")
            .expect("Failed to parse static datetime. Test broken.")
            .to_utc(),
        zp_modifiziert: Some(
            chrono::DateTime::parse_from_rfc3339("1950-01-01T22:01:02+00:00")
                .expect("Failed to parse static datetime. Test broken.")
                .to_utc(),
        ),
        trojanergefahr: Some(2u8),
        schlagworte: vec![],
        touched_by: vec![],
        stellungnahmen: vec![models::DokumentOrApiId::Dokument(Box::new(
            default_stellungnahme(),
        ))],
        additional_links: vec!["https://example.com/videos/aus/der/hoelle".to_string()],
        dokumente: vec![models::DokumentOrApiId::Dokument(Box::new(
            default_dokument(),
        ))],
        gremium: default_gremium(),
        meta_tiebreaker: None,
    }
}
pub(crate) fn default_gremium() -> models::Gremium {
    models::Gremium {
        link: Some("https://a.xyz".to_string()),
        name: "Ausschuss für Inneres und Gemüsaufläufe".to_string(),
        parlament: models::Parlament::Bt,
        wahlperiode: 20,
    }
}
pub(crate) fn default_dokument() -> models::Dokument {
    models::Dokument{
                api_id: Some(uuid::uuid!("b18bde64-c0ff-eeee-ff0c-deadbeef3333")),
                autoren: vec![default_autor_person()],
                hash: HashWrapper::Legacy("f98d9d6f136109780d69f6f98d9d6f136109780d69f6".to_string()),
                drucksnr: Some("20/441".to_string()),
                kurztitel: Some("Dokumentblubgedöns".to_string()),
                link: "https://irgendwo.im.nirgendwo.de".to_string(),
                subdoc_id: None,
                meinung: None,
                titel: "Ganz ausführlicher Titel, der die Schuppenfärbungsverordnung von 2027 zu verändern versucht bevor sie Gesetz wird".to_string(),
                typ: models::Doktyp::Entwurf,
                volltext: "Nee, ich denk mir hier keinen Volltext aus. Das wär wirklich viel zu lang. Vor allem zu einer Schuppenfärbeverordnung aus der Zukunft! Soo lächerlich. 
                Natürlich mal wieder Klassiker, dass die hier \"Schuppen\" und nicht \"Fischschuppen\", \"Gartenschuppen\" oder \"Drachenschuppen\" geschrieben haben. Danke Merkel! 
                Ich persönlich ziehen ja eine Drachenschuppenfärbeverordnung einer Gartenschuppenfärbeverordnung in jedem Fall vor...".to_string(),
                vorwort: Some("Vorwort".to_string()),
                zusammenfassung: Some(ZusammenfassungWrapper::Legacy("Zusammenfassungstext kommt hier rein".to_string())),
                schlagworte: vec!["drache".to_string(), "langer text".to_string(), "mächtiggewaltigegon".to_string(), "schuppen".to_string(), "verordnung".to_string()],
                zp_erstellt: Some(chrono::DateTime::parse_from_rfc3339("1950-01-01T22:01:02+00:00")
                    .expect("Failed to parse static datetime. Test broken.")
                    .to_utc()),
                zp_referenz: chrono::DateTime::parse_from_rfc3339("1950-01-01T22:01:02+00:00")
                    .expect("Failed to parse static datetime. Test broken.")
                    .to_utc(),
                zp_modifiziert: chrono::DateTime::parse_from_rfc3339("1950-01-01T22:01:02+00:00")
                    .expect("Failed to parse static datetime. Test broken.")
                    .to_utc(),
                touched_by: vec![],
            }
}
pub(crate) fn default_stellungnahme() -> models::Dokument {
    models::Dokument{
                api_id: Some(uuid::uuid!("b18bde64-c0ff-eeee-ff0c-deadbeef7777")),
                autoren: vec![default_autor_person()],
                hash: HashWrapper::Legacy("f98d9d6f136109780d69f6f98d9d6f13635463450d69f6".to_string()),
                drucksnr: None,
                subdoc_id: None,
                kurztitel: Some("Dokumentblubgedöns".to_string()),
                link: "https://irgendwo.im.nirgendwo.de".to_string(),
                meinung: Some(3u8),
                titel: "Stelungnahme zu: Ganz ausführlicher Titel, der die Schuppenfärbungsverordnung von 2027 zu verändern versucht bevor sie Gesetz wird".to_string(),
                typ: models::Doktyp::Stellungnahme,
                volltext: "Nee, ich denk mir hier keinen Volltext aus. Das wär wirklich viel zu lang. Vor allem zu einer Schuppenfärbeverordnung aus der Zukunft! Soo lächerlich. 
                Natürlich mal wieder Klassiker, dass die hier \"Schuppen\" und nicht \"Fischschuppen\", \"Gartenschuppen\" oder \"Drachenschuppen\" geschrieben haben. Danke Merkel! 
                Ich persönlich ziehen ja eine Drachenschuppenfärbeverordnung einer Gartenschuppenfärbeverordnung in jedem Fall vor...".to_string(),
                vorwort: Some("Stelluingsnahmenvorwort das völlig verschieden von dem Hauptdokument ist".to_string()),
                zusammenfassung: Some(ZusammenfassungWrapper::Legacy("Zusammenfassungstext kommt hier rein".to_string())),
                schlagworte: vec!["drache".to_string(), "langer text".to_string(), "mächtiggewaltigegon".to_string(), "schuppen".to_string(), "verordnung".to_string()],
                zp_erstellt: Some(chrono::DateTime::parse_from_rfc3339("1950-01-01T22:01:02+00:00")
                    .expect("Failed to parse static datetime. Test broken.")
                    .to_utc()),
                zp_referenz: chrono::DateTime::parse_from_rfc3339("1950-01-01T22:01:02+00:00")
                    .expect("Failed to parse static datetime. Test broken.")
                    .to_utc(),
                zp_modifiziert: chrono::DateTime::parse_from_rfc3339("1950-01-01T22:01:02+00:00")
                    .expect("Failed to parse static datetime. Test broken.")
                    .to_utc(),
                touched_by: vec![],
            }
}
pub(crate) fn default_autor_person() -> models::Autor {
    models::Autor {
        fachgebiet: None,
        lobbyregister: None,
        organisation: "Ministerium der Magie".to_string(),
        person: Some("Harald Maria Töpfer".to_string()),
    }
}
pub(crate) fn default_autor_institution() -> models::Autor {
    models::Autor {
        fachgebiet: None,
        lobbyregister: None,
        organisation: "Mysterium der Ministerien".to_string(),
        person: None,
    }
}

pub(crate) fn default_autor_lobby() -> models::Autor {
    models::Autor {
        fachgebiet: None,
        lobbyregister: Some(
            "https://lobbyregister.beispiel/heinzpeter-karlsbader-ff878f".to_string(),
        ),
        organisation: "Kachelofenzerstörung Heinzelfrau".to_string(),
        person: Some("Heinz-Peter Karlsbader".to_string()),
    }
}

pub(crate) fn default_vorgang() -> models::Vorgang {
    let mut at = vec![default_autor_person(), default_autor_institution()];
    at.sort_by(|a, b| a.organisation.cmp(&b.organisation));
    models::Vorgang {
        api_id: uuid::uuid!("b18bde64-c0ff-eeee-ff0c-deadbeef106e"),
        titel: "Testtitel".to_string(),
        kurztitel: Some("Kurzer Testtitel".to_string()),
        stationen: vec![default_station()],
        typ: models::Vorgangstyp::BuZustimmungInibreg,
        verfassungsaendernd: false,
        wahlperiode: 20,
        touched_by: vec![],
        schlagworte: vec![
            "biowaffen".to_owned(),
            "kabbelei".to_owned(),
            "wasserstoffbombe".to_owned(),
        ],
        links: vec!["https://example.com/ichmagmoneten".to_string()],
        initiatoren: at,
        ids: vec![
            models::VgIdent {
                id: "einzigartig".to_string(),
                typ: "initdrucks".to_owned(),
            },
            models::VgIdent {
                id: "einzigartig".to_string(),
                typ: "zweizigartig".to_owned(),
            },
        ],
        lobbyregister: vec![models::Lobbyregistereintrag {
            betroffene_drucksachen: vec!["20/2014".to_string()],
            intention: "Für die Klicks".to_string(),
            interne_id: "as9d8fja9s8djf".to_string(),
            link: "https://example.com/einig/gerecht/frei".to_string(),
            organisation: default_autor_lobby(),
        }],
        ressort: None,
        sachgebiete: vec![],
        zusammenfassung: vec![models::Zusammenfassungstupel {
            typ: "klimpertod".to_owned(),
            inhalt: "asdfa00dsjf9ajsdpfj ajd f9pajsdpfasdjfjoji toller vorgang paoidcj9pasjdfj"
                .to_owned(),
        }],
    }
}
