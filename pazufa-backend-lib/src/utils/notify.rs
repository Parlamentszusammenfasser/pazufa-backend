use std::fmt::{Display, Write};

use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::{PazufaServer, Result, error::DataValidationError};
use lettre::{
    Message, SmtpTransport, Transport,
    message::{MessageBuilder, header::ContentType},
};
use uuid::Uuid;

#[derive(Copy, Clone)]
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

struct MailQueue {
    mailer: SmtpTransport,
    template: MessageBuilder,
    ambiguous_match: Vec<Mail>,
    variant_added: Vec<Mail>,
    sonstig_unwrapped: Vec<Mail>,
}

impl MailQueue {
    pub fn new(mailer: SmtpTransport, template: MessageBuilder) -> Self {
        Self {
            mailer,
            template,
            ambiguous_match: vec![],
            variant_added: vec![],
            sonstig_unwrapped: vec![],
        }
    }

    pub fn enqueue(&mut self, mail: Mail) {
        match mail.tp {
            MailNotificationType::AmbiguousMatch => self.ambiguous_match.push(mail),
            MailNotificationType::EnumAdded => self.variant_added.push(mail),
            MailNotificationType::SonstigUnwrapped => self.sonstig_unwrapped.push(mail),
        }
    }

    pub fn send(&mut self) {
        for mail_type in [
            MailNotificationType::AmbiguousMatch,
            MailNotificationType::EnumAdded,
            MailNotificationType::SonstigUnwrapped,
        ] {
            self.send_type(mail_type);
        }
    }

    fn send_type(&mut self, mail_type: MailNotificationType) {
        let (mails, typ) = match mail_type {
            MailNotificationType::AmbiguousMatch => (&mut self.ambiguous_match, "ambiguos matches"),
            MailNotificationType::EnumAdded => (&mut self.variant_added, "new variants"),
            MailNotificationType::SonstigUnwrapped => {
                (&mut self.sonstig_unwrapped, "sonstig's unwrapped")
            }
        };
        if mails.is_empty() {
            return;
        }
        let len = mails.len();
        let email = match self
            .template
            .clone()
            .subject(format!("Found {len} {typ} since last check"))
            .body(
                mails
                    .drain(..)
                    .fold(String::new(), |mut a: String, n: Mail| {
                        // Discarding error as writeln!(String, ...) can only fail if the formatter of the
                        // arguments fails, which cannot happen for strings.
                        let _ = writeln!(a, "=======================\n{}\n\n{}", n.subject, n.body);
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
        match self.mailer.send(&email) {
            Ok(_) => tracing::info!("Sent Mail about {len} new {typ}"),
            Err(e) => tracing::error!(
                "Failed to send Mail about {len} new {typ}: Error during transport.\n{e}"
            ),
        }
    }
}

pub struct MailBundle(UnboundedSender<Mail>);

impl MailBundle {
    /// Creates a new mail bundle
    /// # Errors
    /// Can Fail due to:
    /// - missing credentials
    /// - io errors
    pub fn new(config: &crate::Configuration) -> Result<Option<Self>> {
        let (sender, recipient, mailer) = match config.build_mailer() {
            Ok(cm) => cm,
            Err(e) => {
                tracing::warn!(
                    "Failed to create mailer: {}\nMailer will not be available",
                    e
                );
                return Ok(None);
            }
        };

        let (tx, mut rx) = unbounded_channel::<Mail>();

        tokio::spawn(async move {
            let tick_duration = std::time::Duration::from_mins(1);
            let mut state = MailQueue::new(
                mailer,
                Message::builder()
                    .from(sender)
                    .to(recipient)
                    .header(ContentType::TEXT_PLAIN),
            );

            'outer: while let Some(mail) = rx.recv().await {
                state.enqueue(mail);
                'inner: loop {
                    tokio::select! {
                        mail = rx.recv() => {
                            if let Some(mail) = mail { state.enqueue(mail) } else {
                                state.send();
                                break 'outer;
                            }
                        }
                        () = tokio::time::sleep(tick_duration) => {
                            break 'inner;
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

impl PazufaServer {
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
    server: &PazufaServer,
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
    server: &PazufaServer,
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

pub fn notify_unknown_variant<T>(api_id: Uuid, object: &str, server: &PazufaServer) {
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
