/// URL builders for default frontend
use uuid::Uuid;

// Assuming frontend is on the same host as backend
pub fn get_profile_page_url(instance_url: &str, profile_id: &Uuid) -> String {
    format!("{}/profile/{}", instance_url, profile_id)
}

pub fn get_post_page_url(instance_url: &str, post_id: &Uuid) -> String {
    format!("{}/post/{}", instance_url, post_id)
}

pub fn get_tag_page_url(instance_url: &str, tag_name: &str) -> String {
    format!("{}/tag/{}", instance_url, tag_name)
}
