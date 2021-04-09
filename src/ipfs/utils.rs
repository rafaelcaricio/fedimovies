use regex::Regex;

pub const IPFS_LOGO: &str = "bafybeihc4hti5ix4ds2tefhy35qd4c7n5as5cazdmksrxj7ipvcxm64h54";

pub fn get_ipfs_url(cid: &str) -> String {
    format!("ipfs://{}", cid)
}

#[derive(thiserror::Error, Debug)]
#[error("parse error")]
pub struct ParseError;

pub fn parse_ipfs_url(url: &str) -> Result<String, ParseError> {
    let regexp = Regex::new(r"ipfs://(?P<cid>\w+)").unwrap();
    let caps = regexp.captures(&url).ok_or(ParseError)?;
    let cid = caps.name("cid")
        .ok_or(ParseError)?
        .as_str().to_string();
    Ok(cid)
}
