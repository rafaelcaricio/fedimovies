use std::net::{Ipv4Addr, Ipv6Addr};
use url::{Host, ParseError, Url};

pub fn get_hostname(url: &str) -> Result<String, ParseError> {
    let hostname = match Url::parse(url)?
        .host()
        .ok_or(ParseError::EmptyHost)?
    {
        Host::Domain(domain) => domain.to_string(),
        Host::Ipv4(addr) => addr.to_string(),
        Host::Ipv6(addr) => addr.to_string(),
    };
    Ok(hostname)
}

pub fn guess_protocol(hostname: &str) -> &'static str {
    let maybe_ipv4_address = hostname.parse::<Ipv4Addr>();
    if let Ok(_ipv4_address) = maybe_ipv4_address {
        return "http";
    };
    let maybe_ipv6_address = hostname.parse::<Ipv6Addr>();
    if let Ok(_ipv6_address) = maybe_ipv6_address {
        return "http";
    };
    if hostname.ends_with(".onion") || hostname.ends_with(".i2p") {
        // Tor / I2P
        "http"
    } else {
        // Use HTTPS by default
        "https"
    }
}

pub fn normalize_url(url: &str) -> Result<Url, url::ParseError> {
    let normalized_url = if
        url.starts_with("http://") ||
        url.starts_with("https://")
    {
        url.to_string()
    } else {
        // Add scheme
        // Doesn't work for IPv6
        let hostname = if let Some((hostname, _port)) = url.split_once(':') {
            hostname
        } else {
            url
        };
        let url_scheme = guess_protocol(hostname);
        format!(
            "{}://{}",
            url_scheme,
            url,
        )
    };
    let url = Url::parse(&normalized_url)?;
    url.host().ok_or(ParseError::EmptyHost)?; // validates URL
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_hostname() {
        let url = "https://example.org/objects/1";
        let hostname = get_hostname(url).unwrap();
        assert_eq!(hostname, "example.org");
    }

    #[test]
    fn test_get_hostname_if_port_number() {
        let url = "http://127.0.0.1:8380/objects/1";
        let hostname = get_hostname(url).unwrap();
        assert_eq!(hostname, "127.0.0.1");
    }

    #[test]
    fn test_get_hostname_tor() {
        let url = "http://2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion/objects/1";
        let hostname = get_hostname(url).unwrap();
        assert_eq!(hostname, "2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion");
    }

    #[test]
    fn test_get_hostname_yggdrasil() {
        let url = "http://[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be]/objects/1";
        let hostname = get_hostname(url).unwrap();
        assert_eq!(hostname, "319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be");
    }

    #[test]
    fn test_get_hostname_email() {
        let url = "mailto:user@example.org";
        let result = get_hostname(url);
        assert_eq!(result.is_err(), true);
    }

    #[test]
    fn test_guess_protocol() {
        assert_eq!(
            guess_protocol("example.org"),
            "https",
        );
        assert_eq!(
            guess_protocol("2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion"),
            "http",
        );
        assert_eq!(
            guess_protocol("zzz.i2p"),
            "http",
        );
        // Yggdrasil
        assert_eq!(
            guess_protocol("319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be"),
            "http",
        );
        // localhost
        assert_eq!(
            guess_protocol("127.0.0.1"),
            "http",
        );
    }

    #[test]
    fn test_normalize_url() {
        let result = normalize_url("https://test.net").unwrap();
        assert_eq!(result.to_string(), "https://test.net/");
        let result = normalize_url("example.com").unwrap();
        assert_eq!(result.to_string(), "https://example.com/");
        let result = normalize_url("127.0.0.1:8380").unwrap();
        assert_eq!(result.to_string(), "http://127.0.0.1:8380/");
    }
}
