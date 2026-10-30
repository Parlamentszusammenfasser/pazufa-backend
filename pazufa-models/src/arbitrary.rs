use arbitrary::{Result, Unstructured};

pub fn arbitrary_datetime(u: &mut Unstructured) -> Result<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::from_timestamp(
        u.int_in_range(0..=253_396_903_638)?,
        u.int_in_range(0..=1_000_000)?,
    )
    .ok_or(arbitrary::Error::IncorrectFormat)
}

pub fn arbitrary_optdatetime(
    u: &mut Unstructured,
) -> Result<Option<chrono::DateTime<chrono::Utc>>> {
    Ok(chrono::DateTime::from_timestamp(
        u.int_in_range(0..=253_396_903_638)?,
        u.int_in_range(0..=1_000_000)?,
    ))
}
