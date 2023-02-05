use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct RetentionConfig {
    pub extraneous_posts: Option<u32>,
    pub empty_profiles: Option<u32>,
}

#[allow(clippy::derivable_impls)]
impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            extraneous_posts: None,
            empty_profiles: None,
        }
    }
}
