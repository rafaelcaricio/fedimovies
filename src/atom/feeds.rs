use crate::activitypub::identifiers::{local_actor_id, local_object_id};
use crate::config::Instance;
use crate::models::{
    posts::types::Post,
    profiles::types::DbActorProfile,
};
use crate::utils::{
    datetime::get_min_datetime,
    html::{clean_html_all, escape_html},
};

const ENTRY_TITLE_MAX_LENGTH: usize = 75;

fn make_entry(
    instance_url: &str,
    post: &Post,
) -> String {
    let object_id = local_object_id(instance_url, &post.id);
    let content_escaped = escape_html(&post.content);
    let content_cleaned = clean_html_all(&post.content);
    // Use trimmed content for title
    let mut title: String = content_cleaned.chars()
        .take(ENTRY_TITLE_MAX_LENGTH)
        .collect();
    if title.len() == ENTRY_TITLE_MAX_LENGTH &&
            content_cleaned.len() != ENTRY_TITLE_MAX_LENGTH {
        title += "...";
    };
    format!(
        r#"<entry>
    <id>{url}</id>
    <title>{title}</title>
    <updated>{updated_at}</updated>
    <author><name>{author}</name></author>
    <content type="html">{content}</content>
    <link rel="alternate" href="{url}"/>
</entry>"#,
        url=object_id,
        title=title,
        updated_at=post.created_at.to_rfc3339(),
        author=post.author.username,
        content=content_escaped,
    )
}

pub fn get_feed_url(instance_url: &str, username: &str) -> String {
    format!("{}/feeds/{}", instance_url, username)
}

pub fn make_feed(
    instance: &Instance,
    profile: &DbActorProfile,
    posts: Vec<Post>,
) -> String {
    let actor_id = local_actor_id(&instance.url(), &profile.username);
    let actor_name = profile.display_name.as_ref()
        .unwrap_or(&profile.username);
    let actor_address = profile.actor_address(&instance.hostname());
    let feed_url = get_feed_url(&instance.url(), &profile.username);
    let feed_title = format!("{} (@{})", actor_name, actor_address);
    let mut entries = vec![];
    let mut feed_updated_at = get_min_datetime();
    for post in posts {
        let entry = make_entry(&instance.url(), &post);
        entries.push(entry);
        if post.created_at > feed_updated_at {
            feed_updated_at = post.created_at;
        };
    };
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
<id>{id}</id>
<link rel="self" href="{url}"/>
<title>{title}</title>
<updated>{updated_at}</updated>
{entries}
</feed>"#,
        id=actor_id,
        url=feed_url,
        title=feed_title,
        updated_at=feed_updated_at.to_rfc3339(),
        entries=entries.join("\n"),
    )
}
