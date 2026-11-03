use super::APIScope;
use crate::error::{Error, Result};
use argon2::PasswordHasher;
use base64::prelude::*;
use rand::distr::Alphanumeric;
use rand::{RngExt, SeedableRng, rng, rngs::StdRng};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::marker::PhantomData;
use utoipa::ToSchema;

pub const KEY_LENGTH: usize = 128;

// Keytag = [<prefix>+<suffix>], where prefix can be any named ascii string and suffix is randomly
// generated. The suffix length is KEYTAG_LENGTH-prefix.len()
pub const KEYTAG_LENGTH: usize = 16;
pub const KEYTAG_MAX_PREFIX_LENGTH: usize = 10;

pub const SALT_LENGTH: usize = 16;
pub const SLUG_LENGTH: usize = 6;
pub const CRC_LENGTH: usize = 8;
pub const KEY_MATERIAL_LENGTH: usize = KEY_LENGTH - KEYTAG_LENGTH - SLUG_LENGTH - CRC_LENGTH - 2;

#[derive(sqlx::Type, Debug)]
pub struct APIKeyMetadata {
    pub scope: APIScope,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub rotated_for: Option<i32>,
    pub keytag: String,
}

impl APIKeyMetadata {
    #[must_use]
    pub fn is_being_rotated(&self) -> bool {
        self.rotated_for.is_some() && self.expires_at > chrono::Utc::now()
    }
}

#[derive(sqlx::Type, Clone, Copy, Serialize)]
#[sqlx(type_name = "\"HashAlgorithm\"")]
pub enum HashAlgorithm {
    #[sqlx(rename = "sha256")]
    #[serde(rename = "sha256")]
    Sha256,
    #[sqlx(rename = "argon2id")]
    #[serde(rename = "argon2id")]
    Argon2id,
}

impl Display for HashAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Argon2id => write!(f, "argon2id"),
            Self::Sha256 => write!(f, "sha256"),
        }
    }
}

/// A uniquely identifying string that is generated from key data within the database
#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize, ToSchema)]
pub struct KeyTag(pub String);

impl std::convert::From<String> for KeyTag {
    fn from(x: String) -> Self {
        KeyTag(x)
    }
}

impl std::fmt::Display for KeyTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for KeyTag {
    type Err = std::string::ParseError;
    fn from_str(x: &str) -> std::result::Result<Self, Self::Err> {
        std::result::Result::Ok(KeyTag(x.to_string()))
    }
}

impl std::convert::From<KeyTag> for String {
    fn from(x: KeyTag) -> Self {
        x.0
    }
}

impl std::ops::Deref for KeyTag {
    type Target = String;
    fn deref(&self) -> &String {
        &self.0
    }
}

impl std::ops::DerefMut for KeyTag {
    fn deref_mut(&mut self) -> &mut String {
        &mut self.0
    }
}

#[must_use]
#[allow(
    clippy::arithmetic_side_effects,
    reason = "Bounded by 0..KEYTAG_LENGTH"
)]
pub fn generate_keytag(prefix: Option<&str>) -> [char; KEYTAG_LENGTH] {
    let prefix_length = prefix.map_or(0, str::len).min(KEYTAG_MAX_PREFIX_LENGTH);

    let array: Vec<_> = prefix
        .unwrap_or("")
        .chars()
        .take(prefix_length)
        .chain(
            StdRng::from_rng(&mut rng())
                .sample_iter(Alphanumeric)
                .take(KEYTAG_LENGTH - prefix_length)
                .map(char::from)
                .map(|c| {
                    if rng().random_bool(0.5f64) {
                        c.to_ascii_lowercase()
                    } else {
                        c.to_ascii_uppercase()
                    }
                }),
        )
        .collect();

    let array = array.as_slice().try_into();
    match array {
        Ok(array) => array,
        Err(error) => {
            unreachable!("happens when the number of chars != KEYTAG_LENGTH: {error}")
        }
    }
}

/// Fresh random salt of [`SALT_LENGTH`] bytes, as stored in the database.
#[must_use]
pub fn generate_salt() -> Vec<u8> {
    let mut salt = vec![0u8; SALT_LENGTH];
    StdRng::from_rng(&mut rng()).fill(&mut salt[..]);
    salt
}

#[must_use]
fn generate_crc_of_prekey(input: &str) -> u32 {
    const CRC_BEGIN: usize = SLUG_LENGTH + 1 + KEYTAG_LENGTH + 1 + KEY_MATERIAL_LENGTH;
    assert_eq!(input.len(), CRC_BEGIN);
    crc::Crc::<u32>::new(&crc::CRC_32_CKSUM).checksum(&input.bytes().collect::<Vec<_>>())
}

#[must_use]
pub fn check_crc(key: &str) -> bool {
    const CRC_BEGIN: usize = SLUG_LENGTH + 1 + KEYTAG_LENGTH + 1 + KEY_MATERIAL_LENGTH;
    let subkey = &key[..CRC_BEGIN];
    let crc =
        crc::Crc::<u32>::new(&crc::CRC_32_CKSUM).checksum(&subkey.bytes().collect::<Vec<_>>());
    format!("{crc:08x}").eq_ignore_ascii_case(&key[CRC_BEGIN..])
}

impl<T: APIKeyGenerator> TryFrom<String> for APIKey<T> {
    type Error = Error;
    fn try_from(thing: String) -> Result<APIKey<T>> {
        let key = APIKey(thing, PhantomData);
        if key.is_format_valid() {
            Ok(key)
        } else {
            Err(Error::ValidationError {
                field: "APIKey".to_owned(),
                message: "Key Format invalid or checksum failed".to_owned(),
            })
        }
    }
}

pub trait APIKeyGenerator: Sized {
    #[must_use]
    fn generate(keytag: &[char; KEYTAG_LENGTH]) -> APIKey<Self>;

    /// Hash the key material of an api key according to the generator implementing the method
    /// # Errors
    /// Errors out if the hash generation fails
    fn hash(api_key: &APIKey<Self>, salt: &[u8]) -> Result<String>;

    #[must_use]
    fn hash_algorithm() -> HashAlgorithm;
}

#[derive(Deserialize)]
#[serde(remote = "Self")]
pub struct APIKey<T>(String, PhantomData<T>);

impl<T: APIKeyGenerator> Display for APIKey<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl<T: APIKeyGenerator> std::fmt::Debug for APIKey<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}
impl<T: APIKeyGenerator> Default for APIKey<T> {
    fn default() -> Self {
        let mut rand_iter = StdRng::from_rng(&mut rng())
            .sample_iter(&Alphanumeric)
            .map(char::from)
            .map(|c| {
                if rng().random_bool(0.5f64) {
                    c.to_ascii_lowercase()
                } else {
                    c.to_ascii_uppercase()
                }
            });
        let mut keytag = ['a'; KEYTAG_LENGTH];
        for ktc in &mut keytag {
            let Some(c) = rand_iter.next() else {
                unreachable!("Iterator is a generator");
            };
            *ktc = c;
        }

        Self::new(&keytag)
    }
}

impl<T: APIKeyGenerator> APIKey<T> {
    #[must_use]
    pub fn new(keytag: &[char; KEYTAG_LENGTH]) -> Self {
        T::generate(keytag)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Hash the key material of an api key according to the generator implementing the method
    /// # Errors
    /// Errors out if the hash generation fails
    pub fn hash(&self, salt: &[u8]) -> Result<String> {
        T::hash(self, salt)
    }
    #[must_use]
    pub fn slug(&self) -> &str {
        &self.0[..SLUG_LENGTH]
    }
    #[must_use]
    pub fn keytag(&self) -> &str {
        const PREFIX: usize = SLUG_LENGTH + 1;
        &self.0[PREFIX..PREFIX + KEYTAG_LENGTH]
    }
    #[must_use]
    pub fn key_material(&self) -> &str {
        const PREFIX: usize = SLUG_LENGTH + 1 + KEYTAG_LENGTH + 1;
        &self.0[PREFIX..PREFIX + KEY_MATERIAL_LENGTH]
    }
    #[must_use]
    pub fn crc(&self) -> &str {
        const PREFIX: usize = SLUG_LENGTH + 1 + KEYTAG_LENGTH + 1 + KEY_MATERIAL_LENGTH;
        &self.0[PREFIX..PREFIX + CRC_LENGTH]
    }
    #[must_use]
    pub fn is_format_valid(&self) -> bool {
        self.0.len() == KEY_LENGTH && self.0.starts_with("pazufa-") && check_crc(&self.0)
    }

    #[must_use]
    pub fn hash_algorithm(&self) -> HashAlgorithm {
        T::hash_algorithm()
    }
}

pub struct NoopKeyGenerator;
pub struct Argon2KeyGenerator;
pub struct Sha256KeyGenerator;

impl APIKeyGenerator for NoopKeyGenerator {
    fn hash(_password: &APIKey<Self>, _salt: &[u8]) -> Result<String> {
        panic!(
            "You called the API key with the No-Op generator. This is only meant for format checking"
        )
    }
    fn generate(_keytag: &[char; KEYTAG_LENGTH]) -> APIKey<Self> {
        panic!(
            "You called the API key with the No-Op generator. This is only meant for format checking"
        )
    }
    fn hash_algorithm() -> HashAlgorithm {
        panic!(
            "You called the API key with the No-Op generator. This is only meant for format checking"
        )
    }
}

impl APIKeyGenerator for Sha256KeyGenerator {
    fn hash(password: &APIKey<Self>, salt: &[u8]) -> Result<String> {
        let mut buf = salt.to_vec();
        buf.extend_from_slice(password.key_material().as_bytes());

        Ok(crate::sha256_hex(buf))
    }

    fn generate(keytag: &[char; KEYTAG_LENGTH]) -> APIKey<Self> {
        let pre_key: String = "pazufa-"
            .chars()
            .chain(keytag.iter().copied())
            .chain(std::iter::once('-'))
            .chain(
                StdRng::from_rng(&mut rng())
                    .sample_iter(&Alphanumeric)
                    .take(KEY_MATERIAL_LENGTH)
                    .map(char::from),
            )
            .collect();
        APIKey(
            format!("{pre_key}{:08x}", generate_crc_of_prekey(&pre_key)),
            PhantomData,
        )
    }
    fn hash_algorithm() -> HashAlgorithm {
        HashAlgorithm::Sha256
    }
}

impl APIKeyGenerator for Argon2KeyGenerator {
    fn hash(password: &APIKey<Self>, salt: &[u8]) -> Result<String> {
        let ag2_context = argon2::Argon2::default();
        let raw_bytes: Vec<u8> = BASE64_URL_SAFE_NO_PAD.decode(password.key_material())?;
        let pwh = ag2_context
            .hash_password_with_salt(&raw_bytes, salt)?
            .to_string();
        Ok(pwh)
    }
    fn generate(keytag: &[char; KEYTAG_LENGTH]) -> APIKey<Self> {
        let mut byte_array = vec![0; KEY_MATERIAL_LENGTH / 4 * 3];
        StdRng::from_rng(&mut rng()).fill(&mut byte_array);
        let material_string: String = BASE64_URL_SAFE_NO_PAD.encode(&mut byte_array);
        assert_eq!(material_string.len(), KEY_MATERIAL_LENGTH);

        let pre_key: String = "pazufa-"
            .chars()
            .chain(keytag.iter().copied())
            .chain(std::iter::once('-'))
            .chain(material_string.chars())
            .collect();
        APIKey(
            format!("{pre_key}{:08x}", generate_crc_of_prekey(&pre_key)),
            PhantomData,
        )
    }
    fn hash_algorithm() -> HashAlgorithm {
        HashAlgorithm::Argon2id
    }
}

/// Requests a new API key. The key will be saved as hash in the database and only produced once in clear text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(remote = "Self")]
pub struct APIKeyCreate {
    /// Note: inline enums are not fully supported by openapi-generator
    pub scope: APIScope,

    /// The expiration date of the API Key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,

    /// Keytag name, maximal length is 10 Characters, the rest of the keytag will be filled with
    /// random data. It is recommended to use this to later identify the keys more easily
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keytag_prefix: Option<String>,
}

impl std::fmt::Display for APIScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Admin => "admin",
                Self::Collector => "collector",
                Self::KeyAdder => "keyadder",
            }
        )
    }
}

#[cfg(test)]
mod test {
    use super::HashAlgorithm;
    use super::Sha256KeyGenerator;
    #[allow(
        deprecated,
        reason = "used to check equality with deprecated functionality"
    )]
    use legacy_util_auth_functions as util_auth;

    #[allow(
        deprecated,
        reason = "is explicitly testing equivalence with deprecated function"
    )]
    #[test]
    fn old_and_new_hashes_match() {
        let (key, salt) = util_auth::new_key();
        let legacy_hash = util_auth::hash_full_key(&salt, &key, HashAlgorithm::Sha256);
        let new_key: super::APIKey<Sha256KeyGenerator> = key.clone().try_into().expect("");

        let new_hash = new_key.hash(salt.as_bytes()).expect("");
        assert_eq!(legacy_hash, new_hash);
    }

    /// Stored key hashes are compared as strings, so hashing must stay bit-for-bit stable.
    /// The expected values were produced with argon2 0.5.3 / password-hash 0.5.0;
    /// if this fails, existing API keys in the database stop authenticating.
    #[test]
    fn hashes_match_stored_values() {
        use super::{APIKey, Argon2KeyGenerator};
        const RANDOM_SALT: &[u8] = &[
            0x5a, 0x01, 0xff, 0x80, 0x00, 0x7f, 0x13, 0x37, 0xde, 0xad, 0xbe, 0xef, 0x42, 0x24,
            0x99, 0x10,
        ];
        const LEGACY_SALT: &[u8] = b"ltzf_defaultsalt";

        let argon2: APIKey<Argon2KeyGenerator> = "pazufa-goldenSidGAtoAzQ-KMdT4lNiABA_aPqulWsd5HEMK3WH3fpeUNK_9T9avw95xQ5yeTwVVyj6SX2vlUT1uyLM3l-vC86OSVh4wl3kf0XqzD_reKxHc98b7b54"
            .to_owned()
            .try_into()
            .expect("valid key");
        let sha256: APIKey<Sha256KeyGenerator> = "pazufa-goldendv8bU2xol3-QdGhwMv60nU3nsvkl4ETcz3ar2WxpGmHjU08lrVncerXd3vztSaVnk5kM5JkkWkP8eTWs3aqBivEYKHr7mT2rdvSiCUpyUT0174e9a8a"
            .to_owned()
            .try_into()
            .expect("valid key");

        assert_eq!(
            argon2.hash(RANDOM_SALT).expect("hash"),
            "$argon2id$v=19$m=19456,t=2,p=1$WgH/gAB/Ezferb7vQiSZEA$QMptNDbvUTurnlbCQcbwqIDudi5Y6H5RnJczhHvN50k"
        );
        assert_eq!(
            argon2.hash(LEGACY_SALT).expect("hash"),
            "$argon2id$v=19$m=19456,t=2,p=1$bHR6Zl9kZWZhdWx0c2FsdA$/pRlfCihLVCSvh8117bZk5HcE7kG6wSKpxfjLJo9FAc"
        );
        assert_eq!(
            sha256.hash(RANDOM_SALT).expect("hash"),
            "099cf7b6be5422d7e94be19bc1c0303535bc61f5d5102135114327912d0a19e2"
        );
        assert_eq!(
            sha256.hash(LEGACY_SALT).expect("hash"),
            "b9fa36b854d0a7e6bd680bed85c1d1a7286a8350ddc2f052d4b4a716e39ce008"
        );
    }

    #[test]
    fn generated_salts_are_random_and_sized() {
        let (a, b) = (super::generate_salt(), super::generate_salt());
        assert_eq!(a.len(), super::SALT_LENGTH);
        assert_ne!(a, b);
    }

    #[deprecated(since = "0.3.3")]
    #[allow(deprecated, reason = "is deprecated itself")]
    mod legacy_util_auth_functions {
        use crate::auth::{
            HashAlgorithm, KEY_MATERIAL_LENGTH, KEYTAG_LENGTH, SALT_LENGTH, SLUG_LENGTH,
        };
        use argon2::PasswordHasher;
        use rand::distr::Alphanumeric;
        use rand::rngs::StdRng;
        use rand::{RngExt, SeedableRng, rng};

        #[must_use]
        pub fn hash_full_key(salt: &str, full_key: &str, algo: HashAlgorithm) -> String {
            hash_secret(
                salt,
                &full_key
                    .chars()
                    .skip(SLUG_LENGTH + 1 + KEYTAG_LENGTH + 1)
                    .take(KEY_MATERIAL_LENGTH)
                    .collect::<String>(),
                algo,
            )
        }

        /// Hashes a secret with a bad algo
        /// # Panics
        /// can panic if the encoding does not allocate enough space fore the bytes
        #[must_use]
        pub fn hash_secret(salt: &str, secret: &str, algo: HashAlgorithm) -> String {
            match algo {
                HashAlgorithm::Sha256 => {
                    crate::sha256_hex(salt.chars().chain(secret.chars()).collect::<String>())
                }
                HashAlgorithm::Argon2id => {
                    let argon2_params = argon2::Argon2::default();
                    argon2_params
                        .hash_password_with_salt(secret.as_bytes(), salt.as_bytes())
                        .expect("")
                        .to_string()
                }
            }
        }

        #[must_use]
        pub fn generate_api_key() -> String {
            let keytag = StdRng::from_rng(&mut rng())
                .sample_iter(&Alphanumeric)
                .take(KEYTAG_LENGTH)
                .map(char::from)
                .map(|c| {
                    if rng().random_bool(0.5f64) {
                        c.to_ascii_lowercase()
                    } else {
                        c.to_ascii_uppercase()
                    }
                });

            let key_material = StdRng::from_rng(&mut rng())
                .sample_iter(&Alphanumeric)
                .take(KEY_MATERIAL_LENGTH)
                .map(char::from)
                .map(|c| {
                    if rng().random_bool(0.5f64) {
                        c.to_ascii_lowercase()
                    } else {
                        c.to_ascii_uppercase()
                    }
                });

            let pre_key: String = "pazufa-"
                .chars()
                .chain(keytag)
                .chain(std::iter::once('-'))
                .chain(key_material)
                .collect();

            let crc_result = crc::Crc::<u32>::new(&crc::CRC_32_CKSUM)
                .checksum(&pre_key.bytes().collect::<Vec<_>>());
            let key = format!("{pre_key}{crc_result:08x}");
            key
        }
        #[must_use]
        pub fn generate_salt() -> String {
            StdRng::from_rng(&mut rng())
                .sample_iter(&Alphanumeric)
                .take(SALT_LENGTH)
                .map(char::from)
                .map(|c| {
                    if rng().random_bool(0.5f64) {
                        c.to_ascii_lowercase()
                    } else {
                        c.to_ascii_uppercase()
                    }
                })
                .collect()
        }

        /// generates a new key within the database, returning the new key and the new salt.
        #[must_use]
        pub fn new_key() -> (String, String) {
            let new_key = generate_api_key();
            let new_salt = generate_salt();

            (new_key, new_salt)
        }
    }
}
