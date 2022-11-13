use rand::Rng;

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
