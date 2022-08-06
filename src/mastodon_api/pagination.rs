use actix_web::HttpResponse;
use serde::Serialize;

fn get_pagination_header(
    instance_url: &str,
    path: &str,
    last_id: &str,
) -> String {
    let next_page_url = format!(
        "{}{}?max_id={}",
        instance_url,
        path,
        last_id
    );
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
        let pagination_header = get_pagination_header(
            instance_url,
            path,
            &last_item_id.to_string(),
        );
        HttpResponse::Ok()
            .append_header(("Link", pagination_header))
            .json(items)
    } else {
        HttpResponse::Ok().json(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANCE_URL: &str = "https://example.org";

    #[test]
    fn test_get_next_page_link() {
        let result = get_pagination_header(
            INSTANCE_URL,
            "/api/v1/notifications",
            "123",
        );
        assert_eq!(
            result,
            r#"<https://example.org/api/v1/notifications?max_id=123>; rel="next""#,
        );
    }
}
