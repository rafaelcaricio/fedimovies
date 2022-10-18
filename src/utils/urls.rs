use std::net::Ipv6Addr;
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
    let maybe_ipv6_address = hostname.parse::<Ipv6Addr>();
    if let Ok(ipv6_address) = maybe_ipv6_address {
        let prefix = ipv6_address.segments()[0];
        if prefix >= 0x0200 && prefix <= 0x03ff {
            // Yggdrasil
            return "http";
        };
    };
    if hostname.ends_with(".onion") || hostname.ends_with(".i2p") {
        // Tor / I2P
        "http"
    } else {
        // Use HTTPS by default
        "https"
    }
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
        assert_eq!(
            guess_protocol("319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be"),
            "http",
        );
    }
}
