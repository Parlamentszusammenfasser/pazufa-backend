use anyhow::Result;
use pazufa_backend_lib::models;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

type TSType = chrono::DateTime<chrono::Utc>;
const PER_PAGE_MAX: u32 = 10;

pub async fn index_vorgang(
    last_mod: TSType,
    tx: Sender<Vec<models::Vorgang>>,
    db_pool: Arc<PgPool>,
) -> Result<()> {
    tracing::info!("Indexing Vorgang from Database");
    // while page in pages
    let mut i: u32 = 0;
    loop {
        tracing::debug!("Loop {i}");
        i = i.saturating_add(1);
        let mut ta = db_pool.begin().await?;
        // fetch all vorgang objects after last_mod
        let (_, vector) = pazufa_backend_lib::db::retrieve::vorgang_by_parameter(
            pazufa_backend_lib::db::retrieve::VGGetParameters {
                lower_date: Some(last_mod),
                upper_date: None,
                parlament: None,
                wp: None,
                inipsn: None,
                iniorg: None,
                inifch: None,
                vgtyp: None,
            },
            Some(i),
            Some(PER_PAGE_MAX),
            true,
            &mut ta,
        )
        .await?;
        tracing::debug!("Loop {i}");
        ta.commit().await?;
        if vector.is_empty() {
            break;
        }
        tracing::info!("Found a batch of {}", vector.len());
        // send each page into sender
        tx.send(vector).await?;
    }
    Ok(())
}
pub async fn index_sitzung(
    last_mod: TSType,
    tx: Sender<Vec<models::Sitzung>>,
    db_pool: Arc<PgPool>,
) -> Result<()> {
    tracing::info!("Indexing Sitzungen from Database");
    let mut i: u32 = 0;
    loop {
        i = i.saturating_add(1);
        let mut ta = db_pool.begin().await?;
        // fetch all vorgang objects after last_mod
        let (_, vector) = pazufa_backend_lib::db::retrieve::sitzung_by_parameter(
            &pazufa_backend_lib::db::retrieve::SitzungFilterParameters {
                since: Some(last_mod),
                until: None,
                parlament: None,
                wp: None,
                vgid: None,
                gremium_like: None,
            },
            Some(i),
            Some(PER_PAGE_MAX),
            true,
            &mut ta,
        )
        .await?;
        ta.commit().await?;
        if vector.is_empty() {
            break;
        }
        tracing::info!("Found a batch of {}", vector.len());
        // send each page into sender
        tx.send(vector).await?;
    }
    Ok(())
}
pub async fn index_dokument(
    last_mod: TSType,
    tx: Sender<Vec<models::Dokument>>,
    db_pool: Arc<PgPool>,
) -> Result<()> {
    tracing::info!("Indexing Dokumente from Database");
    let mut i: u32 = 0;
    loop {
        i = i.saturating_add(1);
        let mut ta = db_pool.begin().await?;
        // fetch all vorgang objects after last_mod
        let (_, vector) = pazufa_backend_lib::db::retrieve::get_dokument_by_parameter(
            pazufa_backend_lib::db::retrieve::DokumentRetrivalParameters {
                last_mod: Some(last_mod),
                page: Some(i),
                per_page: Some(PER_PAGE_MAX),
            },
            &mut ta,
        )
        .await?;
        ta.commit().await?;
        if vector.is_empty() {
            break;
        }
        tracing::info!("Found a batch of {}", vector.len());
        // send each page into sender
        tx.send(vector).await?;
    }
    Ok(())
}
