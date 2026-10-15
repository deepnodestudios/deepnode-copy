use serde::{Deserialize, Serialize};
use xxhash_rust::xxh3::Xxh3;
use blake3::Hasher as Blake3Hasher;
use sha2::{Sha256, Digest};
use sha1::Sha1;
use md5::Md5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum HashAlgorithm {
    #[default]
    XxHash3_64,
    XxHash3_128,
    Blake3,
    Md5,
    Sha1,
    Sha256,
}

#[allow(clippy::large_enum_variant)]
pub enum MultiHasher {
    XxHash3_64(Xxh3),
    XxHash3_128(Xxh3),
    Blake3(Blake3Hasher),
    Md5(Md5),
    Sha1(Sha1),
    Sha256(Sha256),
}

impl MultiHasher {
    pub fn new(algo: HashAlgorithm) -> Self {
        match algo {
            HashAlgorithm::XxHash3_64 => MultiHasher::XxHash3_64(Xxh3::new()),
            HashAlgorithm::XxHash3_128 => MultiHasher::XxHash3_128(Xxh3::new()),
            HashAlgorithm::Blake3 => MultiHasher::Blake3(Blake3Hasher::new()),
            HashAlgorithm::Md5 => MultiHasher::Md5(Md5::new()),
            HashAlgorithm::Sha1 => MultiHasher::Sha1(Sha1::new()),
            HashAlgorithm::Sha256 => MultiHasher::Sha256(Sha256::new()),
        }
    }

    #[inline]
    pub fn update(&mut self, data: &[u8]) {
        match self {
            MultiHasher::XxHash3_64(h) => h.update(data),
            MultiHasher::XxHash3_128(h) => h.update(data),
            MultiHasher::Blake3(h) => {
                h.update(data);
            }
            MultiHasher::Md5(h) => h.update(data),
            MultiHasher::Sha1(h) => sha1::Digest::update(h, data),
            MultiHasher::Sha256(h) => sha2::Digest::update(h, data),
        }
    }

    pub fn finalize_hex(self) -> String {
        match self {
            MultiHasher::XxHash3_64(h) => format!("{:016x}", h.digest()),
            MultiHasher::XxHash3_128(h) => format!("{:032x}", h.digest128()),
            MultiHasher::Blake3(h) => h.finalize().to_hex().to_string(),
            MultiHasher::Md5(h) => hex::encode(h.finalize()),
            MultiHasher::Sha1(h) => hex::encode(sha1::Digest::finalize(h)),
            MultiHasher::Sha256(h) => hex::encode(sha2::Digest::finalize(h)),
        }
    }
}
