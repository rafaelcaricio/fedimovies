use rsa::pkcs8::{FromPrivateKey, FromPublicKey, ToPrivateKey, ToPublicKey};
use rsa::{Hash, PaddingScheme, PublicKey, RsaPrivateKey, RsaPublicKey};
use sha2::{Digest, Sha256};

pub fn generate_rsa_key() -> Result<RsaPrivateKey, rsa::errors::Error> {
    let mut rng = rand::rngs::OsRng;
    let bits = 2048;
    RsaPrivateKey::new(&mut rng, bits)
}

#[cfg(feature = "test-utils")]
pub fn generate_weak_rsa_key() -> Result<RsaPrivateKey, rsa::errors::Error> {
    use rand::SeedableRng;
    let mut rng = rand::rngs::SmallRng::seed_from_u64(0);
    let bits = 512;
    RsaPrivateKey::new(&mut rng, bits)
}

pub fn serialize_private_key(private_key: &RsaPrivateKey) -> Result<String, rsa::pkcs8::Error> {
    private_key.to_pkcs8_pem().map(|val| val.to_string())
}

pub fn deserialize_private_key(private_key_pem: &str) -> Result<RsaPrivateKey, rsa::pkcs8::Error> {
    RsaPrivateKey::from_pkcs8_pem(private_key_pem)
}

pub fn get_public_key_pem(private_key: &RsaPrivateKey) -> Result<String, rsa::pkcs8::Error> {
    let public_key = RsaPublicKey::from(private_key);
    public_key.to_public_key_pem()
}

pub fn deserialize_public_key(public_key_pem: &str) -> Result<RsaPublicKey, rsa::pkcs8::Error> {
    // rsa package can't decode PEM string with non-standard wrap width,
    // so the input should be normalized first
    let parsed_pem =
        pem::parse(public_key_pem.trim().as_bytes()).map_err(|_| rsa::pkcs8::Error::Pem)?;
    let normalized_pem = pem::encode(&parsed_pem);
    RsaPublicKey::from_public_key_pem(&normalized_pem)
}

/// RSASSA-PKCS1-v1_5 signature
pub fn create_rsa_sha256_signature(
    private_key: &RsaPrivateKey,
    message: &str,
) -> Result<Vec<u8>, rsa::errors::Error> {
    let digest = Sha256::digest(message.as_bytes());
    let padding = PaddingScheme::new_pkcs1v15_sign(Some(Hash::SHA2_256));
    let signature = private_key.sign(padding, &digest)?;
    Ok(signature)
}

/// RSASSA-PKCS1-v1_5 signature
pub fn create_rsa_signature(
    private_key: &RsaPrivateKey,
    message: &str,
) -> Result<Vec<u8>, rsa::errors::Error> {
    let digest = Sha256::digest(message.as_bytes());
    let padding = PaddingScheme::new_pkcs1v15_sign(Some(Hash::SHA2_256));
    let signature = private_key.sign(padding, &digest)?;
    Ok(signature)
}

pub fn get_message_digest(message: &str) -> String {
    let digest = Sha256::digest(message.as_bytes());
    let digest_b64 = base64::encode(digest);
    digest_b64
}

pub fn verify_rsa_sha256_signature(
    public_key: &RsaPublicKey,
    message: &str,
    signature: &[u8],
) -> bool {
    let digest = Sha256::digest(message.as_bytes());
    let padding = PaddingScheme::new_pkcs1v15_sign(Some(Hash::SHA2_256));
    let is_valid = public_key.verify(padding, &digest, signature).is_ok();
    is_valid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_public_key_nowrap() {
        let public_key_pem = "-----BEGIN PUBLIC KEY-----\nMIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQC8ehqQ7n6+pw19U8q2UtxE/9017STW3yRnnqV5nVk8LJ00ba+berqwekxDW+nw77GAu3TJ+hYeeSerUNPup7y3yO3V
YsFtrgWDQ/s8k86sNBU+Ce2GOL7seh46kyAWgJeohh4Rcrr23rftHbvxOcRM8VzYuCeb1DgVhPGtA0xULwIDAQAB\n-----END PUBLIC KEY-----";
        let result = deserialize_public_key(public_key_pem);
        assert!(result.is_ok());
    }

    #[test]
    fn test_public_key_serialization_deserialization() {
        let private_key = generate_weak_rsa_key().unwrap();
        let public_key_pem = get_public_key_pem(&private_key).unwrap();
        let public_key = deserialize_public_key(&public_key_pem).unwrap();
        assert_eq!(public_key, RsaPublicKey::from(&private_key));
    }

    #[test]
    fn test_verify_rsa_signature() {
        let private_key = generate_weak_rsa_key().unwrap();
        let message = "test".to_string();
        let signature = create_rsa_sha256_signature(&private_key, &message).unwrap();
        let public_key = RsaPublicKey::from(&private_key);

        let is_valid = verify_rsa_sha256_signature(&public_key, &message, &signature);
        assert!(is_valid);
    }
}
