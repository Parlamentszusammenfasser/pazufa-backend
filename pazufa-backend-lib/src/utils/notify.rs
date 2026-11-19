use std::fmt::Display;

use crate::{PazufaServer, Result, error::DataValidationError};
use uuid::Uuid;

impl PazufaServer {
    /// guarded to String conversion
    pub fn guard_ts<T: ToString>(&self, input: &T, api_id: Uuid, object: &str) -> String {
        let temp = input.to_string();
        if temp == "sonstig" {
            notify_unknown_variant::<T>(api_id, object);
        }
        temp
    }
}

pub fn notify_new_enum_entry<T: std::fmt::Debug + Display>(new_entry: &T, similarity: &[(f32, T)]) {
    let subject = format!(
        "Für Typ `{}` wurde ein neuer Eintrag `{:?}` erstellt. ",
        std::any::type_name::<T>(),
        new_entry
    );

    let simstr = similarity
        .iter()
        .map(|(p, t)| format!("{p}: {t}"))
        .fold(String::new(), |a, n| format!("{a}\n{n}"));

    let body = format!("Es gibt {} ähnliche Einträge: {simstr}", similarity.len());
    tracing::warn!("Notify: New Enum Entry: {subject}\n{body}!");
}

/// Notifies of a match that has more than one candidate
/// # Errors
/// Can fail due to mailer errors if mail is configured
pub fn notify_ambiguous_match<T: std::fmt::Debug + serde::Serialize>(
    api_ids: &[Uuid],
    object: &T,
    during_operation: &str,
) -> Result<()> {
    let subject = format!("Ambiguous Match: Während {during_operation}");
    let body = format!(
        "Während: `{during_operation}` wurde folgendes Objekt wurde hochgeladen: {}.
        Folgende Objekte in der Datenbank sind ähnlich: {:#?}",
        serde_json::to_string_pretty(object).map_err(|e| DataValidationError::InvalidFormat {
            field: "passed obj for ambiguous match".to_string(),
            message: e.to_string()
        })?,
        api_ids
    );
    tracing::error!("Notify: Ambiguous Match: {subject}\n{body}");
    Ok(())
}

pub fn notify_unknown_variant<T>(api_id: Uuid, object: &str) {
    let subject = format!(
        "Für {object} `{api_id}` wurde `sonstig` angegeben als Wert für `{}`",
        std::any::type_name::<T>()
    );
    tracing::warn!("Notify: Unknown Variant in Guarded Enumeration Field: {subject}");
}
