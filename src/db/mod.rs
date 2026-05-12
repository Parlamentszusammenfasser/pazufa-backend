use uuid::Uuid;

pub mod delete;
pub mod insert;
pub mod merge;
pub mod retrieve;

#[cfg(feature = "track-validation")]
pub mod validate;

pub(crate) type KeyIndex = i32;

fn gen_identifier(scraper_id: Uuid) -> Uuid {
    Uuid::new_v5(
        &scraper_id,
        &rand::random_iter().take(64).collect::<Vec<_>>(),
    )
}
