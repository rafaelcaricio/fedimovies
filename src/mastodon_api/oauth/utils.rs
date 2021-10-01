use base64;
use rand;
use rand::prelude::*;

const ACCESS_TOKEN_SIZE: usize = 20;

pub fn generate_access_token() -> String {
    let mut rng = rand::thread_rng();
    let value: [u8; ACCESS_TOKEN_SIZE] = rng.gen();
    base64::encode_config(value, base64::URL_SAFE_NO_PAD)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_access_token() {
        let token = generate_access_token();
        assert!(token.len() > ACCESS_TOKEN_SIZE);
    }
}
