use crate::Config;
use anyhow::anyhow;
use anyhow::{Result, bail};
use base64::prelude::*;
use pazufa_backend_lib::models;
use serde::Serialize;
use std::io::Write;

pub struct EsClient {
    client: reqwest::Client,
    auth_header: String,
    base_url: String,
}
pub trait FetchId {
    fn fetch_id(&self) -> Result<uuid::Uuid>;
}
#[derive(Serialize)]
struct BatchControlLine {
    _index: String,
    _id: String,
}
#[derive(Serialize)]
struct BatchLine {
    index: BatchControlLine,
}

impl EsClient {
    pub async fn new(base_url: &str, config: &Config) -> Result<Self> {
        let auth_header = match (&config.es_user, &config.es_pass, &config.es_apikey) {
            (Some(_), Some(_), Some(_)) => bail!(
                "Ambiguous authentication Credentials: Please use either user/password or api key based auth"
            ),
            (Some(u), Some(p), None) => {
                format!("Basic {}", BASE64_STANDARD.encode(format!("{u}:{p}")))
            }
            (None, None, Some(api_key)) => {
                format!("ApiKey {api_key}")
            }
            _ => bail!("Incomplete Authentication Credentials"),
        };
        let this = Self {
            client: reqwest::Client::new(),
            base_url: base_url.to_owned(),
            auth_header,
        };
        this.client.get(base_url).send().await?;
        Ok(this)
    }
    pub async fn batch_index<T: Serialize + Sized + Sync + Send + FetchId>(
        &self,
        index_name: &'static str,
        batch: Vec<T>,
    ) -> Result<()> {
        tracing::info!("Submitting batch to elasticsearch, index `{index_name}`");
        let url = format!("{}/{index_name}/_bulk", self.base_url);
        let mut batch_content = std::io::BufWriter::new(Vec::new());
        for t in batch {
            // always use index
            let bl = BatchLine {
                index: BatchControlLine {
                    _index: index_name.to_string(),
                    _id: t.fetch_id()?.to_string(),
                },
            };
            // shape the objects into es batch format
            let _ = writeln!(batch_content, "{}", serde_json::to_string(&bl)?);
            let _ = writeln!(batch_content, "{}", serde_json::to_string(&t)?);
        }
        let output = String::from_utf8(batch_content.into_inner()?)?;
        // construct request
        let _response = self
            .client
            .request(reqwest::Method::PUT, url)
            .header("Content-Type", "application/x-ndjson")
            .header("Authorization", &self.auth_header)
            .body(output)
            .send()
            .await?;
        // add authentication
        // send
        Ok(())
    }
}

impl FetchId for models::Vorgang {
    fn fetch_id(&self) -> Result<uuid::Uuid> {
        Ok(self.api_id)
    }
}
impl FetchId for models::Sitzung {
    fn fetch_id(&self) -> Result<uuid::Uuid> {
        self.api_id.ok_or(anyhow!(
            "Error: Backend should assign all objects an api id"
        ))
    }
}
impl FetchId for models::Dokument {
    fn fetch_id(&self) -> Result<uuid::Uuid> {
        self.api_id.ok_or(anyhow!(
            "Error: Backend should assign all objects an api id"
        ))
    }
}
