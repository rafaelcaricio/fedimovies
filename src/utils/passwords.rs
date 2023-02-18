use super::random::generate_random_sequence;

pub fn hash_password(password: &str) -> Result<String, argon2::Error> {
    let salt: [u8; 32] = generate_random_sequence();
    let config = argon2::Config::default();

    argon2::hash_encoded(password.as_bytes(), &salt, &config)
}

pub fn verify_password(
    password_hash: &str,
    password: &str,
) -> Result<bool, argon2::Error> {
    argon2::verify_encoded(password_hash, password.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_password() {
        let password = "$test123";
        let password_hash = hash_password(password).unwrap();
        let result = verify_password(&password_hash, password);
        assert_eq!(result.is_ok(), true);
    }
}
