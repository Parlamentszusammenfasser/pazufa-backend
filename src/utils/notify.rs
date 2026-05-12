use std::fmt::{Display, Write};

use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::{LTZFServer, Result, error::DataValidationError};
use lettre::{Message, Transport, message::header::ContentType};
use uuid::Uuid;

#[non_exhaustive]
enum MailNotificationType {
    EnumAdded,
    SonstigUnwrapped,
    AmbiguousMatch,
}
struct Mail {
    subject: String,
    body: String,
    tp: MailNotificationType,
}

pub struct MailBundle(UnboundedSender<Mail>);

impl MailBundle {
    /// Creates a new mail bundle
    /// # Errors
    /// Can Fail due to:
    /// - missing credentials
    /// - io errors
    pub fn new(config: &crate::Configuration) -> Result<Option<Self>> {
        let mailer = match config.build_mailer() {
            Ok(cm) => cm,
            Err(e) => {
                tracing::warn!(
                    "Failed to create mailer: {}\nMailer will not be available",
                    e
                );
                return Ok(None);
            }
        };

        let Some(mail_sender) = &config.mail_sender else {
            tracing::warn!(
                "Failed to create mailer: Missing Mail Sender.\nMailer will not be available",
            );
            return Ok(None);
        };
        let sender = lettre::message::Mailbox::new(
            Some("PaZuFa".to_owned()),
            mail_sender
                .parse()
                .map_err(|e| DataValidationError::InvalidFormat {
                    field: "mail address".to_string(),
                    message: format!("{e}"),
                })?,
        );
        let Some(mail_recipient) = &config.mail_recipient else {
            tracing::warn!(
                "Failed to create mailer: Missing Mail Recipient.\nMailer will not be available",
            );
            return Ok(None);
        };
        let recipient: lettre::message::Mailbox =
            mail_recipient
                .parse()
                .map_err(|e| DataValidationError::InvalidFormat {
                    field: "mail address".to_string(),
                    message: format!("{e}"),
                })?;

        let (tx, mut rx) = unbounded_channel::<Mail>();

        tokio::spawn(async move {
            let mut tick_interval = tokio::time::interval(std::time::Duration::from_secs(20));
            let sender = sender;
            let recipient = recipient;
            let mut ambiguous_match = vec![];
            let mut variant_added = vec![];
            let mut sonstig_unwrapped = vec![];

            let send_mail_inner = |mails: &mut Vec<Mail>, typ: &str| {
                let len = mails.len();
                let email = match Message::builder()
                    .from(sender.clone())
                    .to(recipient.clone())
                    .subject(format!("Found {len} {typ} since last check"))
                    .header(ContentType::TEXT_PLAIN)
                    .body(
                        mails
                            .drain(..)
                            .fold(String::new(), |mut a: String, n: Mail| {
                                // Discarding error as writeln!(String, ...) can only fail if the formatter of the
                                // arguments fails, which cannot happen for strings.
                                let _ = writeln!(
                                    a,
                                    "=======================\n{}\n\n{}",
                                    n.subject, n.body
                                );
                                a
                            }),
                    ) {
                    Ok(mail) => mail,
                    Err(e) => {
                        tracing::error!(
                            "Failed to send Mail about {len} new {typ}: Failed to build message.\n{e}"
                        );
                        return;
                    }
                };
                match mailer.send(&email) {
                    Ok(_) => tracing::info!("Sent Mail about {len} new {typ}"),
                    Err(e) => tracing::error!(
                        "Failed to send Mail about {len} new {typ}: Error during transport.\n{e}"
                    ),
                }
            };

            let send_mail = |ambiguous_match: &mut Vec<Mail>,
                             variant_added: &mut Vec<Mail>,
                             sonstig_unwrapped: &mut Vec<Mail>| {
                if !ambiguous_match.is_empty() {
                    send_mail_inner(ambiguous_match, "ambiguos matches");
                }
                if !variant_added.is_empty() {
                    send_mail_inner(variant_added, "new variants");
                }
                if !sonstig_unwrapped.is_empty() {
                    send_mail_inner(sonstig_unwrapped, "sonstig's unwrapped");
                }
            };

            loop {
                tokio::select! {
                    _ = tick_interval.tick() => {
                        send_mail(&mut ambiguous_match, &mut variant_added, &mut sonstig_unwrapped);
                    },
                    mail = rx.recv() => {
                        if let Some(mail) = mail {
                            match mail.tp {
                                MailNotificationType::AmbiguousMatch =>
                                    ambiguous_match.push(mail),
                                MailNotificationType::EnumAdded =>
                                    variant_added.push(mail),
                                MailNotificationType::SonstigUnwrapped =>
                                    sonstig_unwrapped.push(mail),
                            }
                        } else {
                            // Either received None because all sender closed, i.e. the Bundle dropped.
                            send_mail(&mut ambiguous_match, &mut variant_added, &mut sonstig_unwrapped);
                            break;
                        }
                    }
                }
            }
        });
        Ok(Some(Self(tx)))
    }
    fn send(&self, mail: Mail) {
        // Dropping errors as this only happens, if the sender drops, which we hold.
        let _ = self.0.send(mail);
    }
}

impl LTZFServer {
    /// guarded to String conversion
    pub fn guard_ts<T: ToString>(&self, input: &T, api_id: Uuid, object: &str) -> String {
        let temp = input.to_string();
        if temp == "sonstig" {
            notify_unknown_variant::<T>(api_id, object, self);
        }
        temp
    }
}

pub fn notify_new_enum_entry<T: std::fmt::Debug + Display>(
    new_entry: &T,
    similarity: &[(f32, T)],
    server: &LTZFServer,
) {
    let Some(mailbundle) = &server.mailbundle else {
        return;
    };
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
    mailbundle.send(Mail {
        subject,
        body,
        tp: MailNotificationType::EnumAdded,
    });
}

pub fn notify_ambiguous_match<T: std::fmt::Debug + serde::Serialize>(
    api_ids: &[Uuid],
    object: &T,
    during_operation: &str,
    server: &LTZFServer,
) -> Result<()> {
    let Some(mailbundle) = &server.mailbundle else {
        return Ok(());
    };
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
    tracing::error!("Notify: Ambiguous Match!");
    mailbundle.send(Mail {
        subject,
        body,
        tp: MailNotificationType::AmbiguousMatch,
    });
    Ok(())
}

pub fn notify_unknown_variant<T>(api_id: Uuid, object: &str, server: &LTZFServer) {
    let Some(mailbundle) = &server.mailbundle else {
        return;
    };
    let subject = format!(
        "Für {object} `{api_id}` wurde `sonstig` angegeben als Wert für `{}`",
        std::any::type_name::<T>()
    );
    tracing::warn!("Notify: Unknown Variant in Guarded Enumeration Field");
    mailbundle.send(Mail {
        subject,
        body: String::new(),
        tp: MailNotificationType::SonstigUnwrapped,
    });
}
