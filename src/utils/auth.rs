use rand::distr::Alphanumeric;
use rand::{Rng, rng};
use sha256::digest;

pub const KEY_LENGTH: usize = 128;
pub const KEYTAG_LENGTH: usize = 16;
pub const SALT_LENGTH: usize = 16;
pub const SLUG_LENGTH: usize = 6;
pub const CRC_LENGTH: usize = 8;
pub const KEY_MATERIAL_LENGTH: usize = KEY_LENGTH - KEYTAG_LENGTH - SLUG_LENGTH - CRC_LENGTH - 2;

#[must_use]
pub fn keytag_of(thing: &str) -> String {
    thing
        .chars()
        .skip(SLUG_LENGTH + 1)
        .take(KEYTAG_LENGTH)
        .collect()
}
#[must_use]
pub fn hash_full_key(salt: &str, full_key: &str) -> String {
    hash_secret(
        salt,
        &full_key
            .chars()
            .skip(SLUG_LENGTH + 1 + KEYTAG_LENGTH + 1)
            .take(KEY_MATERIAL_LENGTH)
            .collect::<String>(),
    )
}
#[must_use]
pub fn hash_secret(salt: &str, secret: &str) -> String {
    digest(salt.chars().chain(secret.chars()).collect::<String>())
}
#[must_use]
pub fn generate_api_key() -> String {
    let keytag = rng()
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

    let key_material = rng()
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

    let crc_result =
        crc::Crc::<u32>::new(&crc::CRC_32_CKSUM).checksum(&pre_key.bytes().collect::<Vec<_>>());
    let key = format!("{pre_key}{crc_result:08x}");
    key
}
#[must_use]
pub fn generate_salt() -> String {
    rng()
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
#[must_use]
pub fn check_crc(key: &str) -> bool {
    let subkey = &key[..120];
    let crc =
        crc::Crc::<u32>::new(&crc::CRC_32_CKSUM).checksum(&subkey.bytes().collect::<Vec<_>>());
    format!("{crc:08x}") == key[120..]
}
#[must_use]
pub fn check_key(key: &str) -> bool {
    key.len() == KEY_LENGTH && key.starts_with("pazufa-") && check_crc(key)
}

/// generates a new key within the database, returning the new key and the new salt.
#[must_use]
pub fn new_key() -> (String, String) {
    let new_key = generate_api_key();
    let new_salt = generate_salt();

    (new_key, new_salt)
}
#[cfg(test)]
mod test {
    #[test]
    fn test_keygen() {
        let key = super::generate_api_key();
        assert_eq!(key.len(), super::KEY_LENGTH);
        assert!(super::check_crc(&key));
    }
}
