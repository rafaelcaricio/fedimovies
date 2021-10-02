use hex;
use rand;
use rand::prelude::*;

const INVITE_CODE_LENGTH: usize = 32;

pub fn generate_invite_code() -> String {
    let mut rng = rand::thread_rng();
    let value: [u8; INVITE_CODE_LENGTH / 2] = rng.gen();
    hex::encode(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_invite_code() {
        let invite_code = generate_invite_code();
        assert_eq!(invite_code.len(), INVITE_CODE_LENGTH);
    }
}
