/// https://docs.ipfs.io/reference/http/api/

use reqwest::{multipart, Client};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all="PascalCase")]
struct ObjectAdded {
    hash: String,
}

/// Add file to IPFS.
/// Returns CID v1 of the object.
pub async fn add(ipfs_api_url: &str, data: Vec<u8>) -> Result<String, reqwest::Error> {
    let client = Client::new();
    let file_part = multipart::Part::bytes(data);
    let form = multipart::Form::new().part("file", file_part);
    let url = format!("{}/api/v0/add", ipfs_api_url);
    let response = client.post(&url)
        .query(&[("cid-version", 1)])
        .multipart(form)
        .send()
        .await?;
    let info: ObjectAdded = response.json().await?;
    Ok(info.hash)
}
