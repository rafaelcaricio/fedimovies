use rand;
use rand::prelude::*;
use rsa::{Hash, PaddingScheme, PublicKey, RsaPrivateKey, RsaPublicKey};
use rsa::pkcs8::{FromPrivateKey, FromPublicKey, ToPrivateKey, ToPublicKey};
use sha2::{Digest, Sha256};

pub fn hash_password(password: &str) -> Result<String, argon2::Error> {
    let mut rng = rand::thread_rng();
    let salt: [u8; 32] = rng.gen();
    let config = argon2::Config::default();

    argon2::hash_encoded(password.as_bytes(), &salt, &config)
}

pub fn verify_password(
    password_hash: &str,
    password: &str,
) -> Result<bool, argon2::Error> {
    argon2::verify_encoded(password_hash, password.as_bytes())
}

pub fn generate_private_key() -> Result<RsaPrivateKey, rsa::errors::Error> {
    let mut rng = rand::rngs::OsRng;
    let bits = 2048;
    RsaPrivateKey::new(&mut rng, bits)
}

pub fn serialize_private_key(
    private_key: RsaPrivateKey,
) -> Result<String, rsa::pkcs8::Error> {
    private_key.to_pkcs8_pem().map(|val| val.to_string())
}

pub fn deserialize_private_key(
    private_key_pem: &str,
) -> Result<RsaPrivateKey, rsa::pkcs8::Error> {
    RsaPrivateKey::from_pkcs8_pem(private_key_pem)
}

pub fn get_public_key_pem(
    private_key: &RsaPrivateKey,
) -> Result<String, rsa::pkcs8::Error> {
    let public_key = RsaPublicKey::from(private_key);
    public_key.to_public_key_pem()
}

pub fn deserialize_public_key(
    public_key_pem: &str,
) -> Result<RsaPublicKey, rsa::pkcs8::Error> {
    RsaPublicKey::from_public_key_pem(public_key_pem.trim())
}

pub fn sign_message(
    private_key: &RsaPrivateKey,
    message: &str,
) -> Result<String, rsa::errors::Error> {
    let digest = Sha256::digest(message.as_bytes());
    let padding = PaddingScheme::new_pkcs1v15_sign(Some(Hash::SHA2_256));
    let signature = private_key.sign(padding, &digest)?;
    let signature_b64 = base64::encode(&signature);
    Ok(signature_b64)
}

pub fn get_message_digest(message: &str) -> String {
    let digest = Sha256::digest(message.as_bytes());
    let digest_b64 = base64::encode(digest);
    format!("SHA-256={}", digest_b64)
}

pub fn verify_signature(
    public_key: &RsaPublicKey,
    message: &str,
    signature_b64: &str,
) -> Result<bool, base64::DecodeError> {
    let digest = Sha256::digest(message.as_bytes());
    let padding = PaddingScheme::new_pkcs1v15_sign(Some(Hash::SHA2_256));
    let signature = base64::decode(signature_b64)?;
    let is_valid = public_key.verify(
        padding,
        &digest,
        &signature,
    ).is_ok();
    Ok(is_valid)
}

#[cfg(test)]
mod tests {
    use rand::rngs::OsRng;
    use super::*;

    #[test]
    fn test_public_key_serialization_deserialization() {
        let private_key = RsaPrivateKey::new(&mut OsRng, 512).unwrap();
        let public_key_pem = get_public_key_pem(&private_key).unwrap();
        let public_key = deserialize_public_key(&public_key_pem).unwrap();
        assert_eq!(public_key, RsaPublicKey::from(&private_key));
    }

    #[test]
    fn test_verify_signature() {
        let private_key = RsaPrivateKey::new(&mut OsRng, 512).unwrap();
        let message = "test".to_string();
        let signature = sign_message(&private_key, &message).unwrap();
        let public_key = RsaPublicKey::from(&private_key);

        let is_valid = verify_signature(&public_key, &message, &signature).unwrap();
        assert_eq!(is_valid, true);
    }
}
