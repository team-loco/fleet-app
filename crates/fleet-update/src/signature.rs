use std::fmt::Write as _;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};
use sha2::{Digest as _, Sha256};

use crate::{Error, Result};

pub const PUBLIC_KEY: &str = "UEpMz/32EGmOL+jvM+S2xGnA2R2Ih7Z+ujOYCwZ+3gY=";
pub const SIGNATURE_EXTENSION: &str = "sig";
pub const CHECKSUM_EXTENSION: &str = "sha256";
const SEED_LEN: usize = 32;

#[derive(Debug)]
pub struct Keypair {
    pub private: String,
    pub public: String,
}

pub fn generate() -> Result<Keypair> {
    let mut seed = [0u8; SEED_LEN];
    getrandom::fill(&mut seed).map_err(|error| Error::Random(error.to_string()))?;
    let signing = SigningKey::from_bytes(&seed);
    seed.fill(0);
    let public = STANDARD.encode(signing.verifying_key().as_bytes());
    let private = STANDARD.encode(signing.as_bytes());
    Ok(Keypair { private, public })
}

fn signing_key(private: &str) -> Result<SigningKey> {
    let bytes = STANDARD.decode(private.trim()).map_err(|_| Error::Key)?;
    let seed: [u8; SEED_LEN] = bytes.try_into().map_err(|_| Error::Key)?;
    Ok(SigningKey::from_bytes(&seed))
}

pub fn public_key(private: &str) -> Result<String> {
    let signing = signing_key(private)?;
    Ok(STANDARD.encode(signing.verifying_key().as_bytes()))
}

pub fn sign(data: &[u8], private: &str) -> Result<String> {
    let signing = signing_key(private)?;
    let signature = signing.sign(data);
    Ok(format!("{}\n", STANDARD.encode(signature.to_bytes())))
}

pub fn verify(data: &[u8], signature: &str, public: &str) -> Result<()> {
    let key_bytes = STANDARD.decode(public.trim()).map_err(|_| Error::Key)?;
    let key_bytes: [u8; SEED_LEN] = key_bytes.try_into().map_err(|_| Error::Key)?;
    let key = VerifyingKey::from_bytes(&key_bytes).map_err(|_| Error::Key)?;
    let signature_bytes = STANDARD
        .decode(signature.trim())
        .map_err(|_| Error::SignatureFormat)?;
    let signature = Signature::from_slice(&signature_bytes).map_err(|_| Error::SignatureFormat)?;
    key.verify_strict(data, &signature)
        .map_err(|_| Error::Signature)
}

pub fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    digest.iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

pub fn checksum_line(data: &[u8], file_name: &str) -> String {
    format!("{}  {file_name}\n", sha256_hex(data))
}

pub fn verify_checksum(data: &[u8], checksum: &str) -> Result<()> {
    let expected = checksum
        .split_whitespace()
        .next()
        .filter(|hex| hex.len() == 2 * Sha256::output_size())
        .filter(|hex| hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or(Error::ChecksumFormat)?;
    if expected.eq_ignore_ascii_case(&sha256_hex(data)) {
        Ok(())
    } else {
        Err(Error::Checksum)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZIP: &[u8] = b"PK\x03\x04 Fleet.app";

    fn keypair() -> Keypair {
        generate().unwrap_or_else(|error| panic!("keygen: {error}"))
    }

    #[test]
    fn a_signature_verifies_with_the_matching_public_key() {
        let keys = keypair();
        let signature = sign(ZIP, &keys.private).unwrap_or_default();
        assert!(verify(ZIP, &signature, &keys.public).is_ok());
        assert_eq!(public_key(&keys.private).ok(), Some(keys.public));
    }

    #[test]
    fn a_tampered_zip_fails_verification() {
        let keys = keypair();
        let signature = sign(ZIP, &keys.private).unwrap_or_default();
        let mut tampered = ZIP.to_vec();
        tampered.push(0);
        assert!(matches!(
            verify(&tampered, &signature, &keys.public),
            Err(Error::Signature)
        ));
    }

    #[test]
    fn a_signature_from_another_key_fails_verification() {
        let signer = keypair();
        let other = keypair();
        let signature = sign(ZIP, &signer.private).unwrap_or_default();
        assert!(matches!(
            verify(ZIP, &signature, &other.public),
            Err(Error::Signature)
        ));
    }

    #[test]
    fn malformed_signatures_and_keys_are_rejected() {
        let keys = keypair();
        assert!(matches!(
            verify(ZIP, "not base64!", &keys.public),
            Err(Error::SignatureFormat)
        ));
        assert!(matches!(
            verify(ZIP, &STANDARD.encode([0u8; 12]), &keys.public),
            Err(Error::SignatureFormat)
        ));
        assert!(matches!(sign(ZIP, "c2hvcnQ="), Err(Error::Key)));
    }

    #[test]
    fn the_embedded_public_key_is_a_valid_ed25519_key() {
        let bytes = STANDARD.decode(PUBLIC_KEY).unwrap_or_default();
        let bytes: Result<[u8; SEED_LEN]> = bytes.try_into().map_err(|_| Error::Key);
        let key = bytes.and_then(|bytes| VerifyingKey::from_bytes(&bytes).map_err(|_| Error::Key));
        assert!(key.is_ok());
    }

    #[test]
    fn checksums_match_the_shasum_format() {
        let line = checksum_line(b"abc", "Fleet-0.2.0.zip");
        assert_eq!(
            line,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  Fleet-0.2.0.zip\n"
        );
        assert!(verify_checksum(b"abc", &line).is_ok());
        assert!(verify_checksum(b"abc", &line.to_uppercase()).is_ok());
        assert!(matches!(
            verify_checksum(b"xyz", &line),
            Err(Error::Checksum)
        ));
        assert!(matches!(
            verify_checksum(b"abc", "zz  Fleet.zip"),
            Err(Error::ChecksumFormat)
        ));
        assert!(matches!(
            verify_checksum(b"abc", ""),
            Err(Error::ChecksumFormat)
        ));
    }
}
