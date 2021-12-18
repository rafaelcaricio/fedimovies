use std::collections::HashMap;

use regex::{Captures, Regex};
use tokio_postgres::GenericClient;

use crate::errors::{DatabaseError, ValidationError};
use crate::models::profiles::queries::get_profiles_by_accts;
use crate::models::profiles::types::DbActorProfile;

const MENTION_RE: &str = r"@?(?P<user>\w+)@(?P<instance>.+)";
const MENTION_SEARCH_RE: &str = r"(?m)(?P<space>^|\s)@(?P<user>\w+)@(?P<instance>\S+)";

fn pattern_to_acct(caps: &Captures, instance_host: &str) -> String {
    if &caps["instance"] == instance_host {
        caps["user"].to_string()
    } else {
        format!("{}@{}", &caps["user"], &caps["instance"])
    }
}

/// Finds everything that looks like a mention
fn find_mentions(
    instance_host: &str,
    text: &str,
) -> Vec<String> {
    let mention_re = Regex::new(MENTION_SEARCH_RE).unwrap();
    let mut mentions = vec![];
    for caps in mention_re.captures_iter(text) {
        let acct = pattern_to_acct(&caps, instance_host);
        if !mentions.contains(&acct) {
            mentions.push(acct);
        };
    };
    mentions
}

pub async fn find_mentioned_profiles(
    db_client: &impl GenericClient,
    instance_host: &str,
    text: &str,
) -> Result<HashMap<String, DbActorProfile>, DatabaseError> {
    let mentions = find_mentions(instance_host, text);
    let profiles = get_profiles_by_accts(db_client, mentions).await?;
    let mut mention_map: HashMap<String, DbActorProfile> = HashMap::new();
    for profile in profiles {
        mention_map.insert(profile.acct.clone(), profile);
    };
    Ok(mention_map)
}

pub fn replace_mentions(
    mention_map: &HashMap<String, DbActorProfile>,
    instance_host: &str,
    instance_url: &str,
    text: &str,
) -> String {
    let mention_re = Regex::new(MENTION_SEARCH_RE).unwrap();
    let result = mention_re.replace_all(text, |caps: &Captures| {
        let acct = pattern_to_acct(caps, instance_host);
        match mention_map.get(&acct) {
            Some(profile) => {
                // Replace with a link to profile.
                // Actor URL may differ from actor ID.
                let url = profile.actor_url(instance_url).unwrap();
                format!(
                    // https://microformats.org/wiki/h-card
                    r#"{}<span class="h-card"><a class="u-url mention" href="{}">@{}</a></span>"#,
                    caps["space"].to_string(),
                    url,
                    profile.username,
                )
            },
            None => caps[0].to_string(), // leave unchanged if actor is not known
        }
    });
    result.to_string()
}

pub fn mention_to_acct(
    instance_host: &str,
    mention: &str,
) -> Result<String, ValidationError> {
    let mention_re = Regex::new(MENTION_RE).unwrap();
    let mention_caps = mention_re.captures(mention)
        .ok_or(ValidationError("invalid mention tag"))?;
    let acct = pattern_to_acct(&mention_caps, instance_host);
    Ok(acct)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

    const INSTANCE_HOST: &str = "server1.com";
    const INSTANCE_URL: &str = "https://server1.com";

    #[test]
    fn test_find_mentions() {
        let text = concat!(
            "@user1@server1.com ",
            "@user2@server2.com ",
            "@@invalid@server2.com ",
            "@test@server3.com@nospace@server4.com ",
            "@notmention ",
            "@user2@server2.com copy ",
            "some text",
        );
        let results = find_mentions(INSTANCE_HOST, text);
        assert_eq!(results, vec![
            "user1",
            "user2@server2.com",
            "test@server3.com@nospace@server4.com",
        ]);
    }

    #[test]
    fn test_replace_mentions() {
        // Local actor
        let profile_1 = DbActorProfile {
            username: "user1".to_string(),
            ..Default::default()
        };
        // Remote actor
        let profile_2 = DbActorProfile {
            username: "user2".to_string(),
            actor_json: Some(json!({
                "id": "https://server2.com/actors/user2",
                "url": "https://server2.com/@user2",
            })),
            ..Default::default()
        };
        let text = "@user1@server1.com @user2@server2.com sometext @notmention @test@unknown.org";
        let mention_map = HashMap::from([
            ("user1".to_string(), profile_1),
            ("user2@server2.com".to_string(), profile_2),
        ]);
        let result = replace_mentions(&mention_map, INSTANCE_HOST, INSTANCE_URL, text);

        let expected_result = concat!(
            r#"<span class="h-card"><a class="u-url mention" href="https://server1.com/users/user1">@user1</a></span> "#,
            r#"<span class="h-card"><a class="u-url mention" href="https://server2.com/@user2">@user2</a></span> "#,
            r#"sometext @notmention @test@unknown.org"#,
        );
        assert_eq!(result, expected_result);
    }

    #[test]
    fn test_mention_to_acct() {
        let mention = "@user@example.com";
        let acct_1 = mention_to_acct("example.com", mention).unwrap();
        assert_eq!(acct_1, "user");

        let acct_2 = mention_to_acct("server.info", mention).unwrap();
        assert_eq!(acct_2, "user@example.com");
    }
}
