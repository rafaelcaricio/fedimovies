/// https://docs.ipfs.io/reference/http/api/
use reqwest::{multipart, Client};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ObjectAdded {
    hash: String,
}

/// Adds file to IPFS.
/// Returns CID v1 of the object.
pub async fn add(ipfs_api_url: &str, data: Vec<u8>) -> Result<String, reqwest::Error> {
    let client = Client::new();
    let file_part = multipart::Part::bytes(data);
    let form = multipart::Form::new().part("file", file_part);
    let url = format!("{}/api/v0/add", ipfs_api_url);
    let response = client
        .post(&url)
        .query(&[("cid-version", 1)])
        .multipart(form)
        .send()
        .await?;
    response.error_for_status_ref()?;
    let info: ObjectAdded = response.json().await?;
    Ok(info.hash)
}

/// Unpins and removes files from local IPFS node.
pub async fn remove(ipfs_api_url: &str, cids: Vec<String>) -> Result<(), reqwest::Error> {
    let client = Client::new();
    let remove_pin_url = format!("{}/api/v0/pin/rm", ipfs_api_url);
    let mut remove_pin_args = vec![];
    for cid in cids {
        log::info!("removing {} from IPFS node", cid);
        remove_pin_args.push(("arg", cid));
    }
    let remove_pin_response = client
        .post(&remove_pin_url)
        .query(&remove_pin_args)
        .query(&[("recursive", true)])
        .send()
        .await?;
    remove_pin_response.error_for_status()?;
    let gc_url = format!("{}/api/v0/repo/gc", ipfs_api_url);
    // Garbage collecting can take a long time
    // https://github.com/ipfs/go-ipfs/issues/7752
    let gc_response = client.post(&gc_url).send().await?;
    gc_response.error_for_status()?;
    Ok(())
}
