use std::collections::HashMap;

use regex::{Captures, Regex};
use tokio_postgres::GenericClient;

use crate::activitypub::actors::types::ActorAddress;
use crate::database::DatabaseError;
use crate::errors::ValidationError;
use crate::models::profiles::queries::get_profiles_by_accts;
use crate::models::profiles::types::DbActorProfile;
use super::links::is_inside_code_block;

// See also: ACTOR_ADDRESS_RE in activitypub::actors::types
const MENTION_SEARCH_RE: &str = r"(?m)(?P<before>^|\s|>|[\(])@(?P<mention>[^\s<]+)";
const MENTION_SEARCH_SECONDARY_RE: &str = r"^(?P<username>[\w\.-]+)(@(?P<hostname>[\w\.-]+\w))?(?P<after>[\.,:?\)]?)$";

/// Finds everything that looks like a mention
fn find_mentions(
    instance_hostname: &str,
    text: &str,
) -> Vec<String> {
    let mention_re = Regex::new(MENTION_SEARCH_RE).unwrap();
    let mention_secondary_re = Regex::new(MENTION_SEARCH_SECONDARY_RE).unwrap();
    let mut mentions = vec![];
    for caps in mention_re.captures_iter(text) {
        let mention_match = caps.name("mention").expect("should have mention group");
        if is_inside_code_block(&mention_match, text) {
            // No mentions inside code blocks
            continue;
        };
        if let Some(secondary_caps) = mention_secondary_re.captures(&caps["mention"]) {
            let username = secondary_caps["username"].to_string();
            let hostname = secondary_caps.name("hostname")
                .map(|match_| match_.as_str())
                .unwrap_or(instance_hostname)
                .to_string();
            let actor_address = ActorAddress { username, hostname };
            let acct = actor_address.acct(instance_hostname);
            if !mentions.contains(&acct) {
                mentions.push(acct);
            };
        };
    };
    mentions
}

pub async fn find_mentioned_profiles(
    db_client: &impl GenericClient,
    instance_hostname: &str,
    text: &str,
) -> Result<HashMap<String, DbActorProfile>, DatabaseError> {
    let mentions = find_mentions(instance_hostname, text);
    // If acct doesn't exist in database, mention is ignored
    let profiles = get_profiles_by_accts(db_client, mentions).await?;
    let mut mention_map: HashMap<String, DbActorProfile> = HashMap::new();
    for profile in profiles {
        mention_map.insert(profile.acct.clone(), profile);
    };
    Ok(mention_map)
}

pub fn replace_mentions(
    mention_map: &HashMap<String, DbActorProfile>,
    instance_hostname: &str,
    instance_url: &str,
    text: &str,
) -> String {
    let mention_re = Regex::new(MENTION_SEARCH_RE).unwrap();
    let mention_secondary_re = Regex::new(MENTION_SEARCH_SECONDARY_RE).unwrap();
    let result = mention_re.replace_all(text, |caps: &Captures| {
        let mention_match = caps.name("mention").expect("should have mention group");
        if is_inside_code_block(&mention_match, text) {
            // Don't replace mentions inside code blocks
            return caps[0].to_string();
        };
        if let Some(secondary_caps) = mention_secondary_re.captures(&caps["mention"]) {
            let username = secondary_caps["username"].to_string();
            let hostname = secondary_caps.name("hostname")
                .map(|match_| match_.as_str())
                .unwrap_or(instance_hostname)
                .to_string();
            let actor_address = ActorAddress { username, hostname };
            let acct = actor_address.acct(instance_hostname);
            if let Some(profile) = mention_map.get(&acct) {
                // Replace with a link to profile.
                // Actor URL may differ from actor ID.
                let url = profile.actor_url(instance_url);
                #[allow(clippy::to_string_in_format_args)]
                return format!(
                    // https://microformats.org/wiki/h-card
                    r#"{}<span class="h-card"><a class="u-url mention" href="{}">@{}</a></span>{}"#,
                    caps["before"].to_string(),
                    url,
                    profile.username,
                    secondary_caps["after"].to_string(),
                );
            };
        };
        // Leave unchanged if actor is not known
        caps[0].to_string()
    });
    result.to_string()
}

pub fn mention_to_address(
    mention: &str,
) -> Result<ActorAddress, ValidationError> {
    // @ prefix is optional
    let actor_address = mention.strip_prefix('@')
        .unwrap_or(mention)
        .parse()?;
    Ok(actor_address)
}

#[cfg(test)]
mod tests {
    use crate::activitypub::actors::types::Actor;
    use super::*;

    const INSTANCE_HOSTNAME: &str = "server1.com";
    const INSTANCE_URL: &str = "https://server1.com";
    const TEXT_WITH_MENTIONS: &str = concat!(
        "@user1 ",
        "@user_x@server1.com,<br>",
        "(@user2@server2.com boosted) ",
        "@user3@server2.com.\n",
        "@@invalid@server2.com ",
        "@test@server3.com@nospace@server4.com ",
        "@ email@unknown.org ",
        "@user2@server2.com copy ",
        "some text",
    );

    #[test]
    fn test_find_mentions() {
        let results = find_mentions(INSTANCE_HOSTNAME, TEXT_WITH_MENTIONS);
        assert_eq!(results, vec![
            "user1",
            "user_x",
            "user2@server2.com",
            "user3@server2.com",
        ]);
    }

    #[test]
    fn test_replace_mentions() {
        // Local actors
        let profile_1 = DbActorProfile {
            username: "user1".to_string(),
            ..Default::default()
        };
        let profile_2 = DbActorProfile {
            username: "user_x".to_string(),
            ..Default::default()
        };
        // Remote actors
        let profile_3 = DbActorProfile {
            username: "user2".to_string(),
            actor_json: Some(Actor {
                id: "https://server2.com/actors/user2".to_string(),
                url: Some("https://server2.com/@user2".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let profile_4 = DbActorProfile {
            username: "user3".to_string(),
            actor_json: Some(Actor {
                id: "https://server2.com/actors/user3".to_string(),
                url: Some("https://server2.com/@user3".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mention_map = HashMap::from([
            ("user1".to_string(), profile_1),
            ("user_x".to_string(), profile_2),
            ("user2@server2.com".to_string(), profile_3),
            ("user3@server2.com".to_string(), profile_4),
        ]);
        let result = replace_mentions(
            &mention_map,
            INSTANCE_HOSTNAME,
            INSTANCE_URL,
            TEXT_WITH_MENTIONS,
        );

        let expected_result = concat!(
            r#"<span class="h-card"><a class="u-url mention" href="https://server1.com/users/user1">@user1</a></span> "#,
            r#"<span class="h-card"><a class="u-url mention" href="https://server1.com/users/user_x">@user_x</a></span>,<br>"#,
            r#"(<span class="h-card"><a class="u-url mention" href="https://server2.com/@user2">@user2</a></span> boosted) "#,
            r#"<span class="h-card"><a class="u-url mention" href="https://server2.com/@user3">@user3</a></span>."#, "\n",
            r#"@@invalid@server2.com @test@server3.com@nospace@server4.com "#,
            r#"@ email@unknown.org <span class="h-card"><a class="u-url mention" href="https://server2.com/@user2">@user2</a></span> copy some text"#,
        );
        assert_eq!(result, expected_result);
    }

    #[test]
    fn test_mention_to_address() {
        let mention = "@user@example.com";
        let address_1 = mention_to_address(mention).unwrap();
        assert_eq!(address_1.acct("example.com"), "user");

        let address_2 = mention_to_address(mention).unwrap();
        assert_eq!(address_2.acct("server.info"), "user@example.com");

        let mention_without_prefix = "user@test.com";
        let address_3 = mention_to_address(mention_without_prefix).unwrap();
        assert_eq!(address_3.to_string(), mention_without_prefix);

        let short_mention = "@user";
        let result = mention_to_address(short_mention);
        assert_eq!(result.is_err(), true);
    }
}
