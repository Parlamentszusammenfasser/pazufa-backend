use crate::{
    DokumentHash, DokumentOrApiId, HashWrapper,
    constraint::HexDigest,
    enumerations::{HashStrategy, Mime},
};
use arbitrary::{Arbitrary, Result, Unstructured};

impl<'a> Arbitrary<'a> for DokumentHash {
    // generate just any random string, but keep the mimes correct
    // and the strings as hex strings.
    // That strings are sha256 hex representations is more of a coincidence
    fn arbitrary(u: &mut Unstructured<'a>) -> Result<Self> {
        let strategy = HashStrategy::arbitrary(u)?;
        let hash_input = char::arbitrary(u)?;
        let res = match strategy {
            HashStrategy::Sha256Text => DokumentHash {
                mime: crate::enumerations::Mime::TextHtml,
                strategy,
                value: crate::sha256_hex(hash_input.to_string()),
            },
            HashStrategy::Sha256Bytes => DokumentHash {
                mime: Mime::ApplicationPdf,
                strategy,
                value: crate::sha256_hex(hash_input.to_string()),
            },
            HashStrategy::Sha1Bytes => DokumentHash {
                mime: Mime::ApplicationPdf,
                strategy,
                // sha1 digests are 40 hex chars long
                value: crate::sha256_hex(hash_input.to_string())
                    .chars()
                    .take(40)
                    .collect(),
            },
        };
        Ok(res)
    }
}

impl<'a> Arbitrary<'a> for HashWrapper {
    fn arbitrary(u: &mut Unstructured<'a>) -> Result<Self> {
        let legacy = u.ratio(1, 2)?;
        if legacy {
            Ok(Self::Legacy(HexDigest::arb(u)?))
        } else {
            // an empty hash list is rejected at deserialization, and the database
            // allows at most one hash per strategy and document
            let len = u.int_in_range(1..=3)?;
            let mut vector: Vec<DokumentHash> = Vec::with_capacity(len);
            for _ in 0..len {
                let hash = DokumentHash::arbitrary(u)?;
                if !vector.iter().any(|h| h.strategy == hash.strategy) {
                    vector.push(hash);
                }
            }
            Ok(Self::Future(vector))
        }
    }
}

impl<'a> Arbitrary<'a> for DokumentOrApiId {
    fn arbitrary(u: &mut Unstructured<'a>) -> Result<Self> {
        Ok(Self::Dokument(Arbitrary::arbitrary(u)?))
    }
}
