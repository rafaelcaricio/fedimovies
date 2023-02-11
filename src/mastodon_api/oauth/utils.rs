use base64;
use rand;
use rand::prelude::*;

pub fn render_authorization_page() -> String {
    let page = r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta http-equiv="X-UA-Compatible" content="IE=edge">
    <meta name="viewport" content="width=device-width,initial-scale=1.0">
    <title>Authorization</title>
    <style nonce="oauth-authorization">
        html, body { height: 100%; }
        form {
            display: flex;
            flex-direction: column;
            gap: 5px;
            margin: auto;
            max-width: 100%;
            position: relative;
            top: 40%;
            width: 200px;
        }
    </style>
</head>
<body>
    <form method="post">
        <input type="text" name="username" placeholder="Username">
        <br>
        <input type="password" name="password" placeholder="Password">
        <br>
        <button type="submit">Submit</button>
    </form>
</body>
</html>
"#.to_string();
    page
}

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
