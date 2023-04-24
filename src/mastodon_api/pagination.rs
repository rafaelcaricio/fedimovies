use actix_web::HttpResponse;
use serde::{Deserialize, Serialize};

fn get_pagination_header(instance_url: &str, path: &str, last_id: &str) -> String {
    let next_page_url = format!("{}{}?max_id={}", instance_url, path, last_id);
    // https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Link
    format!(r#"<{}>; rel="next""#, next_page_url)
}

pub fn get_paginated_response(
    instance_url: &str,
    path: &str,
    items: Vec<impl Serialize>,
    maybe_last_item_id: Option<impl ToString>,
) -> HttpResponse {
    if let Some(last_item_id) = maybe_last_item_id {
        let pagination_header =
            get_pagination_header(instance_url, path, &last_item_id.to_string());
        HttpResponse::Ok()
            .append_header(("Link", pagination_header))
            .json(items)
    } else {
        HttpResponse::Ok().json(items)
    }
}

const PAGE_MAX_SIZE: u16 = 200;

#[derive(Debug, Deserialize)]
#[serde(try_from = "u16")]
pub struct PageSize(u16);

impl PageSize {
    pub fn new(size: u16) -> Self {
        Self(size)
    }

    pub fn inner(&self) -> u16 {
        self.0
    }
}

impl TryFrom<u16> for PageSize {
    type Error = &'static str;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        if value > 0 && value <= PAGE_MAX_SIZE {
            Ok(Self(value))
        } else {
            Err("expected an integer between 0 and 201")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANCE_URL: &str = "https://example.org";

    #[test]
    fn test_get_next_page_link() {
        let result = get_pagination_header(INSTANCE_URL, "/api/v1/notifications", "123");
        assert_eq!(
            result,
            r#"<https://example.org/api/v1/notifications?max_id=123>; rel="next""#,
        );
    }

    #[test]
    fn test_deserialize_page_size() {
        let value: PageSize = serde_json::from_str("10").unwrap();
        assert_eq!(value.inner(), 10);

        let expected_error = "expected an integer between 0 and 201";
        let error = serde_json::from_str::<PageSize>("0").unwrap_err();
        assert_eq!(error.to_string(), expected_error);
        let error = serde_json::from_str::<PageSize>("201").unwrap_err();
        assert_eq!(error.to_string(), expected_error);
    }
}
