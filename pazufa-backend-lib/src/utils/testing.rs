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

    use crate::models::{
        self,
        dokument::{HashWrapper, ZusammenfassungWrapper},
    };

    pub(crate) mod random {

        use crate::models;
        use rand::RngExt;
        use rand::SeedableRng;
        use rand::rngs::StdRng;
        use sha256::digest;

        // Helper function to generate random strings
        pub(crate) fn random_string(
            rng: &mut StdRng,
            prefix: &str,
            min_len: usize,
            max_len: usize,
        ) -> String {
            let len = rng.random_range(min_len..=max_len);
            let mut result = prefix.to_string();
            for _ in 0..len {
                result.push(rng.random_range('a'..='z'));
            }
            result
        }

        // Helper function to randomly choose from enum variants
        pub(crate) fn random_enum<T: Clone>(rng: &mut StdRng, variants: &[T]) -> T {
            variants[rng.random_range(0..variants.len())].clone()
        }

        // Helper function to generate random UUIDs
        pub(crate) fn random_uuid(rng: &mut StdRng) -> uuid::Uuid {
            let mut bytes = [0u8; 16];
            rng.fill(&mut bytes);
            uuid::Uuid::from_bytes(bytes)
        }

        // Helper function to generate random dates
        pub(crate) fn random_date(rng: &mut StdRng) -> chrono::DateTime<chrono::Utc> {
            loop {
                if let Some(dt) = {
                    let year = rng.random_range(2020..=2030);
                    let month = rng.random_range(1..=12);
                    let day = rng.random_range(1..=31);
                    let hour = rng.random_range(0..=23);
                    let minute = rng.random_range(0..=59);
                    let second = rng.random_range(0..=59);

                    chrono::NaiveDate::from_ymd_opt(year, month, day)
                        .and_then(|date| date.and_hms_opt(hour, minute, second))
                        .map(|dt| dt.and_utc())
                } {
                    return dt;
                }
            }
        }

        // Helper function to generate random authors
        pub(crate) fn random_autor(rng: &mut StdRng) -> models::Autor {
            let has_person = rng.random_bool(0.7);
            let has_fachgebiet = rng.random_bool(0.3);
            let has_lobbyregister = rng.random_bool(0.2);

            models::Autor {
                person: if has_person {
                    Some(random_string(rng, "Person_", 5, 15))
                } else {
                    None
                },
                organisation: random_string(rng, "Org_", 8, 25),
                fachgebiet: if has_fachgebiet {
                    Some(random_string(rng, "Fach_", 5, 20))
                } else {
                    None
                },
                lobbyregister: if has_lobbyregister {
                    Some(format!(
                        "https://lobby.{}.de/{}",
                        random_string(rng, "", 3, 8),
                        random_string(rng, "", 8, 16)
                    ))
                } else {
                    None
                },
            }
        }
        pub(crate) fn random_stellungnahme(rnd: &mut StdRng) -> models::Dokument {
            random_base_dokument(rnd, true)
        }
        pub(crate) fn random_dokument(rnd: &mut StdRng) -> models::Dokument {
            random_base_dokument(rnd, false)
        }
        // Helper function to generate random documents
        pub(crate) fn random_base_dokument(rng: &mut StdRng, is_stln: bool) -> models::Dokument {
            let doktyp_variants = [
                models::Doktyp::Entwurf,
                models::Doktyp::Antrag,
                models::Doktyp::Anfrage,
                models::Doktyp::Antwort,
                models::Doktyp::Mitteilung,
                models::Doktyp::Gutachten,
            ];

            let has_drucksnr = rng.random_bool(0.8);
            let has_kurztitel = rng.random_bool(0.6);
            let has_vorwort = rng.random_bool(0.4);
            let has_zusammenfassung = rng.random_bool(0.5);
            let has_zp_erstellt = rng.random_bool(0.6);

            let num_autoren = rng.random_range(1..=3);
            let mut autoren = Vec::new();
            for _ in 0..num_autoren {
                autoren.push(random_autor(rng));
            }

            let schlagworte_count = rng.random_range(0..=5);
            let schlagworte = {
                let mut words = Vec::new();
                for _ in 0..schlagworte_count {
                    words.push(random_string(rng, "tag_", 3, 12));
                }
                words
            };

            models::Dokument {
                api_id: Some(random_uuid(rng)),
                touched_by: vec![],
                subdoc_id: None,
                drucksnr: if has_drucksnr {
                    Some(format!(
                        "{}/{}",
                        rng.random_range(15..=25),
                        rng.random_range(100..=999)
                    ))
                } else {
                    None
                },
                typ: if is_stln {
                    models::Doktyp::Stellungnahme
                } else {
                    random_enum(rng, &doktyp_variants)
                },
                titel: random_string(rng, "Titel für ", 20, 80),
                kurztitel: if has_kurztitel {
                    Some(random_string(rng, "Kurz: ", 10, 40))
                } else {
                    None
                },
                vorwort: if has_vorwort && !is_stln {
                    Some(random_string(rng, "Vorwort: ", 30, 100))
                } else {
                    None
                },
                volltext: random_string(rng, "Volltext des Dokuments: ", 100, 500),
                zusammenfassung: if has_zusammenfassung {
                    Some(ZusammenfassungWrapper::Legacy(random_string(
                        rng,
                        "Zusammenfassung: ",
                        50,
                        200,
                    )))
                } else {
                    None
                },
                zp_modifiziert: random_date(rng),
                zp_referenz: random_date(rng),
                zp_erstellt: if has_zp_erstellt {
                    Some(random_date(rng))
                } else {
                    None
                },
                link: format!(
                    "https://{}.de/dokument/{}",
                    random_string(rng, "", 5, 10),
                    random_string(rng, "", 8, 16)
                ),
                hash: HashWrapper::Legacy(random_string(rng, "", 40, 60)),
                meinung: if is_stln {
                    Some(rng.random_range(1..=5))
                } else {
                    None
                },
                schlagworte,
                autoren,
            }
        }

        // Helper function to generate random stations
        pub(crate) fn random_station(rng: &mut StdRng) -> models::Station {
            let stationstyp_variants = [
                models::Stationstyp::ParlAusschber,
                models::Stationstyp::ParlVollvlsgn,
                models::Stationstyp::ParlAkzeptanz,
                models::Stationstyp::ParlAblehnung,
                models::Stationstyp::ParlZurueckgz,
                models::Stationstyp::ParlGgentwurf,
            ];

            let parlament_variants = [
                models::Parlament::Bt,
                models::Parlament::Br,
                models::Parlament::Bv,
                models::Parlament::Bb,
                models::Parlament::By,
                models::Parlament::Be,
            ];

            let has_titel = rng.random_bool(0.6);
            let has_zp_modifiziert = rng.random_bool(0.7);
            let has_link = rng.random_bool(0.8);
            let has_gremium_federf = rng.random_bool(0.4);
            let has_trojanergefahr = rng.random_bool(0.5);
            let has_additional_links = rng.random_bool(0.3);

            let num_dokumente = rng.random_range(1..=4);
            let mut dokumente = Vec::new();
            for _ in 0..num_dokumente {
                dokumente.push(models::DokumentOrApiId::Dokument(Box::new(
                    random_dokument(rng),
                )));
            }

            let additional_links_count = if has_additional_links {
                rng.random_range(1..=3)
            } else {
                0
            };
            let additional_links = {
                let mut links = Vec::new();
                for _ in 0..additional_links_count {
                    links.push(format!(
                        "https://{}.de/link/{}",
                        random_string(rng, "", 5, 10),
                        random_string(rng, "", 8, 16)
                    ));
                }
                links
            };

            let stellungnahmen_count = rng.random_range(0..=2);
            let stellungnahmen = {
                let mut stn = Vec::new();
                for _ in 0..stellungnahmen_count {
                    stn.push(models::DokumentOrApiId::Dokument(Box::new(
                        random_stellungnahme(rng),
                    )));
                }
                stn
            };

            models::Station {
                api_id: Some(random_uuid(rng)),
                touched_by: vec![],
                titel: if has_titel {
                    Some(random_string(rng, "Station: ", 15, 50))
                } else {
                    None
                },
                zp_start: random_date(rng),
                zp_modifiziert: if has_zp_modifiziert {
                    Some(random_date(rng))
                } else {
                    None
                },
                gremium: models::Gremium {
                    parlament: random_enum(rng, &parlament_variants),
                    wahlperiode: rng.random_range(15..=25),
                    link: if rng.random_bool(0.7) {
                        Some(format!(
                            "https://{}.de/gremium",
                            random_string(rng, "", 5, 10)
                        ))
                    } else {
                        None
                    },
                    name: random_string(rng, "Ausschuss für ", 20, 40),
                },
                gremium_federf: if has_gremium_federf {
                    Some(rng.random_bool(0.5))
                } else {
                    None
                },
                link: if has_link {
                    Some(format!(
                        "https://{}.de/station/{}",
                        random_string(rng, "", 5, 10),
                        random_string(rng, "", 8, 16)
                    ))
                } else {
                    None
                },
                typ: random_enum(rng, &stationstyp_variants),
                trojanergefahr: if has_trojanergefahr {
                    Some(rng.random_range(1..=10))
                } else {
                    None
                },
                schlagworte: vec![],
                dokumente,
                additional_links,
                stellungnahmen,
                meta_tiebreaker: None,
            }
        }
        pub(crate) fn random_vorgang(rng: &mut StdRng) -> models::Vorgang {
            let vorgangstyp_variants = [
                models::Vorgangstyp::BuZustimmungInibreg,
                models::Vorgangstyp::BuEinspruchInibreg,
                models::Vorgangstyp::GgLandParl,
                models::Vorgangstyp::GgLandVolk,
                models::Vorgangstyp::BwEinsatz,
                models::Vorgangstyp::Sonstig,
            ];
            let typ = random_enum(rng, &vorgangstyp_variants);
            vorgang_of_type(rng, typ)
        }
        pub(crate) fn vorgang_of_type(
            rng: &mut StdRng,
            typ: models::Vorgangstyp,
        ) -> models::Vorgang {
            // Generate the main vorgang
            let stat_parls = {
                match typ {
                    models::Vorgangstyp::BuZustimmungInibreg
                    | models::Vorgangstyp::BuEinspruchInibreg => {
                        vec![models::Parlament::Bt, models::Parlament::Br]
                    }
                    models::Vorgangstyp::BwEinsatz => vec![models::Parlament::Bt],
                    _ => vec![
                        models::Parlament::Bb,
                        models::Parlament::Be,
                        models::Parlament::Bw,
                        models::Parlament::By,
                        models::Parlament::Hb,
                        models::Parlament::He,
                        models::Parlament::Hh,
                        models::Parlament::Mv,
                        models::Parlament::Ni,
                        models::Parlament::Nw,
                        models::Parlament::Rp,
                        models::Parlament::Sl,
                        models::Parlament::Sn,
                        models::Parlament::Sh,
                        models::Parlament::St,
                        models::Parlament::Th,
                    ],
                }
            };

            let vgident_typ_variants = ["initdrucks", "vorgnr", "api-id", "sonstig"];

            let has_kurztitel = rng.random_bool(0.6);
            let num_initiatoren = rng.random_range(1..=4);
            let mut initiatoren = Vec::new();
            for _ in 0..num_initiatoren {
                initiatoren.push(random_autor(rng));
            }

            let num_stationen = rng.random_range(1..=5);
            let mut stationen = Vec::new();
            for _ in 0..num_stationen {
                let rstat = random_station(rng);
                stationen.push(models::Station {
                    gremium: models::Gremium {
                        parlament: random_enum(rng, &stat_parls),
                        ..rstat.gremium
                    },
                    ..rstat
                });
            }

            let links_count = rng.random_range(1..=3);
            let links = {
                let mut link_list = Vec::new();
                for _ in 0..links_count {
                    link_list.push(format!(
                        "https://{}.de/vorgang/{}",
                        random_string(rng, "", 5, 10),
                        random_string(rng, "", 8, 16)
                    ));
                }
                link_list
            };

            let ids_count = rng.random_range(0..=3);
            let ids = {
                let mut id_list = Vec::new();
                for _ in 0..ids_count {
                    id_list.push(models::VgIdent {
                        id: random_string(rng, "id_", 5, 15),
                        typ: vgident_typ_variants[rng.random_range(0..vgident_typ_variants.len())]
                            .to_owned(),
                    });
                }
                id_list
            };

            let lobbyregister_count = rng.random_range(1..=2);
            let lobbyregister = {
                let mut lobby_list = Vec::new();
                for _ in 0..lobbyregister_count {
                    let betroffene_count = rng.random_range(1..=3);
                    let mut betroffene = Vec::new();
                    for _ in 0..betroffene_count {
                        betroffene.push(format!(
                            "{}/{}",
                            rng.random_range(15..=25),
                            rng.random_range(100..=999)
                        ));
                    }

                    lobby_list.push(models::Lobbyregistereintrag {
                        organisation: random_autor(rng),
                        interne_id: random_string(rng, "lobby_", 8, 20),
                        intention: random_string(rng, "Intention: ", 20, 60),
                        link: format!(
                            "https://lobby.{}.de/{}",
                            random_string(rng, "", 5, 10),
                            random_string(rng, "", 8, 16)
                        ),
                        betroffene_drucksachen: betroffene,
                    });
                }
                lobby_list
            };
            let schlagworte_count = rng.random_range(0..=5);
            let schlagworte = {
                let mut words = Vec::new();
                for _ in 0..schlagworte_count {
                    words.push(random_string(rng, "tag_", 3, 12));
                }
                words
            };

            let zusammenfassung = {
                let mut tuples = vec![];
                let zusfas_count = schlagworte_count;
                for _ in 0..zusfas_count {
                    tuples.push(models::Zusammenfassungstupel {
                        inhalt: random_string(rng, "zusammenfassung_", 140, 500),
                        typ: random_string(rng, "zftyp_", 3, 12),
                    });
                }
                tuples
            };

            models::Vorgang {
                api_id: random_uuid(rng),
                touched_by: vec![models::TouchedByInner {
                    scraper_id: Some(random_uuid(rng)),
                    key: Some(digest(random_string(rng, "key_", 10, 30))),
                }],
                titel: random_string(rng, "Vorgang: ", 20, 80),
                kurztitel: if has_kurztitel {
                    Some(random_string(rng, "Kurz: ", 10, 40))
                } else {
                    None
                },
                wahlperiode: rng.random_range(15..=25),
                verfassungsaendernd: rng.random_bool(0.2),
                typ,
                schlagworte,
                ids,
                links,
                ressort: None,
                sachgebiete: vec![],
                initiatoren,
                stationen,
                lobbyregister,
                zusammenfassung,
            }
        }
        fn random_sitzung(rng: &mut StdRng) -> models::Sitzung {
            let parlament_variants = [
                models::Parlament::Bb,
                models::Parlament::Be,
                models::Parlament::Br,
                models::Parlament::Bt,
                models::Parlament::Bv,
                models::Parlament::Bw,
                models::Parlament::By,
                models::Parlament::Ek,
                models::Parlament::Hb,
                models::Parlament::He,
                models::Parlament::Hh,
                models::Parlament::Mv,
                models::Parlament::Ni,
                models::Parlament::Nw,
                models::Parlament::Rp,
                models::Parlament::Sh,
                models::Parlament::Sl,
                models::Parlament::Sn,
                models::Parlament::St,
                models::Parlament::Th,
            ];
            models::Sitzung {
                api_id: Some(random_uuid(rng)),
                touched_by: vec![],
                titel: Some(random_string(rng, "", 16, 65)),
                termin: random_date(rng),
                gremium: models::Gremium {
                    name: random_string(rng, "", 10, 20),
                    parlament: random_enum(rng, &parlament_variants),
                    wahlperiode: rng.random_range(15..=25),
                    link: None,
                },
                nummer: rng.random_range(0..=125),
                public: rng.random_bool(0.5),
                link: Some(random_string(rng, "https://", 4, 20)),
                tops: vec![random_top(rng)],
                dokumente: vec![models::DokumentOrApiId::Dokument(Box::new(
                    random_dokument(rng),
                ))],
                experten: vec![random_autor(rng)],
            }
        }
        fn random_top(rng: &mut StdRng) -> models::Top {
            models::Top {
                dokumente: vec![models::DokumentOrApiId::Dokument(Box::new(
                    random_dokument(rng),
                ))],
                nummer: rng.random_range(1..=125),
                titel: random_string(rng, "", 10, 25),
                vorgang_id: vec![],
            }
        }

        pub(crate) fn autor(seed: u64) -> models::Autor {
            let mut rng = StdRng::seed_from_u64(seed);
            random_autor(&mut rng)
        }
        pub(crate) fn station(seed: u64) -> models::Station {
            let mut rng = StdRng::seed_from_u64(seed);
            random_station(&mut rng)
        }
        use crate::models::dokument::HashWrapper;
        use crate::models::dokument::ZusammenfassungWrapper;
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
            let vorgang = vorgang_of_type(&mut rng, t);
            let dates: Vec<_> = vorgang.stationen.iter().map(|x| x.zp_start).collect();

            models::Vorgang {
                stationen: stypes
                    .chars()
                    .map(|c| models::Station {
                        typ: *map
                            .iter()
                            .find(|(_, v)| *v == &c)
                            .expect("expected to find station type in map")
                            .0,
                        gremium: vorgang.stationen[0].gremium.clone(),
                        zp_start: dates[0]
                            .checked_add_days(chrono::Days::new(14))
                            .expect("adding days failed"),
                        ..random_station(&mut rng)
                    })
                    .collect(),
                ..vorgang
            }
        }
        pub(crate) fn vorgang(seed: u64) -> models::Vorgang {
            let mut rng = StdRng::seed_from_u64(seed);
            random_vorgang(&mut rng)
        }
        pub(crate) fn dokument(seed: u64) -> models::Dokument {
            let mut rng = StdRng::seed_from_u64(seed);
            random_dokument(&mut rng)
        }
        pub(crate) fn sitzung(seed: u64) -> models::Sitzung {
            use rand::SeedableRng;
            use rand::rngs::StdRng;
            let mut rng = StdRng::seed_from_u64(seed);
            random_sitzung(&mut rng)
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
