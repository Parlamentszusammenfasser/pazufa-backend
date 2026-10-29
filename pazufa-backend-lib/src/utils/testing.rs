#![allow(
    clippy::disallowed_methods,
    reason = "Database setup and teardown requires use of `sqlx::query` as those cannot be statically verified."
)]
#![allow(
    clippy::too_many_lines,
    reason = "just a testing mod will largely be replaced with arbitrary anyways at some point"
)]
use crate::utils::tracing::Logging;
use crate::{Configuration, PazufaServer};
use sha256::digest;
use sqlx::SqlSafeStr;
use std::sync::LazyLock;

static MASTER_URL: LazyLock<String> = LazyLock::new(|| {
    dotenvy::dotenv().ok();
    std::env::var("DATABASE_URL").expect("Cannot run test without the DATABASE_URL environment variable set. The use of .env files is supported.")
});

pub(crate) struct TestSetup {
    pub(crate) name: &'static str,
    pub(crate) server: PazufaServer,
    create_pool: sqlx::PgPool,
}
impl TestSetup {
    pub(crate) async fn new(dbname: &'static str) -> Self {
        let create_pool = sqlx::PgPool::connect(&MASTER_URL)
            .await
            .expect("Failed to establish connection to database server.");
        let user: String = sqlx::query_scalar::<_, String>("SELECT CURRENT_ROLE;")
            .fetch_one(&create_pool)
            .await
            .expect("Failed to get current user.");
        let testuser = format!("test_{user}");
        sqlx::query(
            sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS {dbname} WITH (FORCE);"))
                .into_sql_str(),
        )
        .execute(&create_pool)
        .await
        .expect("Failed to drop existing database.");
        let _ = sqlx::query(
            sqlx::AssertSqlSafe(format!("CREATE USER \"{testuser}\" with PASSWORD 'test';"))
                .into_sql_str(),
        )
        .execute(&create_pool)
        .await;
        sqlx::query(
            sqlx::AssertSqlSafe(format!("CREATE DATABASE {dbname} WITH OWNER '{testuser}'"))
                .into_sql_str(),
        )
        .execute(&create_pool)
        .await
        .expect("Failed to create test database.");

        let new_pool = match (MASTER_URL.rfind('/'), MASTER_URL.rfind('@')) {
            (Some(slash_pos), Some(at_pos)) => format!(
                "postgres://{testuser}:test@{}/{dbname}",
                &MASTER_URL[at_pos.saturating_add(1)..slash_pos]
            ),
            (Some(slash_pos), None) => {
                let double_slash_pos = MASTER_URL
                    .find("//")
                    .expect("Did not find \"//\" in DATABASE_URL. Invalid URL.");
                format!(
                    "postgres://{testuser}:test@{}/{dbname}",
                    &MASTER_URL[double_slash_pos.saturating_add(2)..slash_pos]
                )
            }
            (None, _) => unreachable!(),
        };

        let pool = sqlx::PgPool::connect(&new_pool)
            .await
            .expect("Failed to establish connection to database server.");
        sqlx::migrate!("../migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations.");
        let hash = digest("total-nutzloser-wert");
        sqlx::query!(
            "INSERT INTO api_keys(key_hash, scope, created_by, salt, keytag, algo)
            VALUES
            ($1, 
            (SELECT id FROM api_scope WHERE value = 'keyadder' LIMIT 1), 
            (SELECT last_value FROM api_keys_id_seq), $2, $3, 1)
            ON CONFLICT DO NOTHING;",
            hash,
            "salziger-salzkeks".as_bytes(),
            "total-nutzlos"
        )
        .execute(&pool)
        .await
        .expect("Failed to create test API keys.");
        let logging = Logging::new("testing_error.log".into(), None);

        Self {
            name: dbname,
            create_pool,
            server: PazufaServer::new(
                pool,
                Configuration {
                    per_object_scraper_log_size: 5,
                    track_path: "tracks.toml".to_owned(),
                    ..Default::default()
                },
                None,
                logging,
            ),
        }
    }
    pub(crate) async fn teardown(&self) {
        sqlx::query(
            sqlx::AssertSqlSafe(format!("DROP DATABASE {} WITH (FORCE);", self.name))
                .into_sql_str(),
        )
        .execute(&self.create_pool)
        .await
        .expect("Failed to drop test database.");
        let _ = std::fs::remove_file("testing_error.log");
    }
}

pub(crate) mod generate {
    use pazufa_models::{
        self as models,
        dokument::{HashWrapper, ZusammenfassungWrapper},
    };

    pub(crate) mod random {
        use arbitrary::{Arbitrary, Unstructured};
        use pazufa_models as models;
        use rand::Rng;
        use rand::SeedableRng;
        use rand::rngs::StdRng;
        #[cfg(feature = "track-validation")]
        use std::collections::BTreeMap;
        #[cfg(feature = "track-validation")]
        pub(crate) fn vorgang_with_stations(
            seed: u64,
            t: models::Vorgangstyp,
            stypes: &str,
            map: &BTreeMap<models::Stationstyp, char>,
        ) -> models::Vorgang {
            let mut rng = StdRng::seed_from_u64(seed);
            let gr: models::Gremium = with_rng(&mut rng);

            models::Vorgang {
                stationen: stypes
                    .chars()
                    .map(|c| models::Station {
                        typ: *map
                            .iter()
                            .find(|(_, v)| *v == &c)
                            .expect("expected to find station type in map")
                            .0,
                        gremium: gr.clone(),
                        zp_start: with_rng(&mut rng),
                        ..with_rng(&mut rng)
                    })
                    .collect(),
                typ: t,
                ..with_rng(&mut rng)
            }
        }

        pub(crate) fn with_rng<T>(rng: &mut StdRng) -> T
        where
            for<'a> T: Arbitrary<'a>,
        {
            let size_hint = T::size_hint(20);

            let mut random_data = vec![0u8; size_hint.1.unwrap_or(size_hint.0) * 8];
            rng.fill_bytes(&mut random_data);
            let unstructured = Unstructured::new(&random_data);
            T::arbitrary_take_rest(unstructured).unwrap_or_else(|e| {
                panic!(
                    "Model Generation failed with this size hint: {size_hint:?} and this error: {e}"
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
    pub(crate) fn default_autor_experte() -> models::Autor {
        models::Autor {
            person: Some("Karl Preis".to_string()),
            organisation: "Kachelofenbau Hannes".to_string(),
            fachgebiet: Some("Kachelofenbau".to_string()),
            lobbyregister: None,
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
    pub(crate) fn default_sitzung() -> models::Sitzung {
        models::Sitzung {
            api_id: Some(uuid::uuid!("b18bde64-c0ff-eeee-ff0c-deadbeef9999")),
            touched_by: vec![],
            titel: Some("Klogespräche und -lektüre im 22. Jhd.".to_string()),
            termin: chrono::DateTime::parse_from_rfc3339("1950-01-01T22:01:02+00:00")
                .expect("Failed to parse static datetime. Test broken.")
                .to_utc(),
            gremium: default_gremium(),
            nummer: 42,
            public: true,
            link: Some("https://klogefueh.le".to_string()),
            tops: vec![default_top()],
            dokumente: vec![models::DokumentOrApiId::Dokument(Box::new(
                default_dokument(),
            ))],
            experten: vec![default_autor_experte()],
        }
    }
    pub(crate) fn default_top() -> models::Top {
        models::Top {
            dokumente: vec![models::DokumentOrApiId::Dokument(Box::new(
                default_dokument(),
            ))],
            nummer: 1,
            titel: "Lektüre und Haptik".to_string(),
            vorgang_id: vec![],
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
}
