use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::Instance;
use crate::errors::ValidationError;
use crate::ethereum::identity::{
    ETHEREUM_EIP191_PROOF,
    DidPkh,
    verify_identity_proof,
};
use crate::models::profiles::types::{ExtraField, IdentityProof};
use crate::models::users::types::User;
use crate::utils::crypto::{deserialize_private_key, get_public_key_pem};
use crate::utils::files::get_file_url;
use super::constants::{ACTOR_KEY_SUFFIX, AP_CONTEXT};
use super::identifiers::{local_actor_id, LocalActorCollection};
use super::vocabulary::{IDENTITY_PROOF, IMAGE, PERSON, PROPERTY_VALUE, SERVICE};

const W3ID_CONTEXT: &str = "https://w3id.org/security/v1";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(test, derive(Default))]
#[serde(rename_all = "camelCase")]
pub struct PublicKey {
    id: String,
    owner: String,
    pub public_key_pem: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Image {
    #[serde(rename = "type")]
    object_type: String,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActorAttachment {
    name: String,

    #[serde(rename = "type")]
    object_type: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    signature_algorithm: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    signature_value: Option<String>,
}

// Clone and Debug traits are required by FromSql
#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(test, derive(Default))]
#[serde(rename_all = "camelCase")]
pub struct Actor {
    #[serde(rename = "@context")]
    pub context: Option<Value>,

    pub id: String,

    #[serde(rename = "type")]
    pub object_type: String,

    pub name: Option<String>,

    pub preferred_username: String,
    pub inbox: String,
    pub outbox: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub followers: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub following: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribers: Option<String>,

    pub public_key: PublicKey,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Image>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<Image>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment: Option<Vec<ActorAttachment>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

fn parse_identity_proof(
    actor_id: &str,
    attachment: &ActorAttachment,
) -> Result<IdentityProof, ValidationError> {
    if attachment.object_type != IDENTITY_PROOF {
        return Err(ValidationError("invalid attachment type"));
    };
    let proof_type = attachment.signature_algorithm.as_ref()
        .ok_or(ValidationError("missing proof type"))?;
    if proof_type != ETHEREUM_EIP191_PROOF {
        return Err(ValidationError("unknown proof type"));
    };
    let did = attachment.name.parse::<DidPkh>()
        .map_err(|_| ValidationError("invalid did"))?;
    let signature = attachment.signature_value.as_ref()
        .ok_or(ValidationError("missing signature"))?;
    verify_identity_proof(
        actor_id,
        &did,
        signature,
    ).map_err(|_| ValidationError("invalid identity proof"))?;
    let proof = IdentityProof {
        issuer: did,
        proof_type: proof_type.to_string(),
        value: signature.to_string(),
    };
    Ok(proof)
}

fn parse_extra_field(
    attachment: &ActorAttachment,
) -> Result<ExtraField, ValidationError> {
    if attachment.object_type != PROPERTY_VALUE {
        return Err(ValidationError("invalid attachment type"));
    };
    let property_value = attachment.value.as_ref()
        .ok_or(ValidationError("missing property value"))?;
    let field = ExtraField {
        name: attachment.name.clone(),
        value: property_value.to_string(),
        value_source: None,
    };
    Ok(field)
}

impl Actor {
    pub fn address(
        &self,
        this_instance_host: &str,
    ) -> Result<ActorAddress, url::ParseError> {
        let actor_host = url::Url::parse(&self.id)?
            .host_str()
            .ok_or(url::ParseError::EmptyHost)?
            .to_owned();
        let is_local = actor_host == this_instance_host;
        let actor_address = ActorAddress {
            username: self.preferred_username.clone(),
            instance: actor_host,
            is_local,
        };
        Ok(actor_address)
    }

    pub fn parse_attachments(&self) -> (Vec<IdentityProof>, Vec<ExtraField>) {
        let mut identity_proofs = vec![];
        let mut extra_fields = vec![];
        if let Some(attachments) = &self.attachment {
            for attachment in attachments {
                match attachment.object_type.as_str() {
                    IDENTITY_PROOF => {
                        match parse_identity_proof(&self.id, attachment) {
                            Ok(proof) => identity_proofs.push(proof),
                            Err(error) => {
                                 log::warn!(
                                    "ignoring actor attachment of type {}: {}",
                                    attachment.object_type,
                                    error,
                                );
                            },
                        };
                    },
                    PROPERTY_VALUE => {
                        match parse_extra_field(attachment) {
                            Ok(field) => extra_fields.push(field),
                            Err(error) => {
                                 log::warn!(
                                    "ignoring actor attachment of type {}: {}",
                                    attachment.object_type,
                                    error,
                                );
                            },
                        };
                    },
                    _ => {
                        log::warn!(
                            "ignoring actor attachment of type {}",
                            attachment.object_type,
                        );
                    },
                };
            };
        };
        (identity_proofs, extra_fields)
    }
}

pub struct ActorAddress {
    pub username: String,
    pub instance: String,
    pub is_local: bool,
}

impl ToString for ActorAddress {
    fn to_string(&self) -> String {
        format!("{}@{}", self.username, self.instance)
    }
}

impl ActorAddress {
    /// Returns acct string, as used in Mastodon
    pub fn acct(&self) -> String {
        if self.is_local {
            self.username.clone()
        } else {
           self.to_string()
        }
    }
}

pub type ActorKeyError = rsa::pkcs8::Error;

pub fn get_local_actor(
    user: &User,
    instance_url: &str,
) -> Result<Actor, ActorKeyError> {
    let username = &user.profile.username;
    let actor_id = local_actor_id(instance_url, username);
    let inbox = LocalActorCollection::Inbox.of(&actor_id);
    let outbox = LocalActorCollection::Outbox.of(&actor_id);
    let followers = LocalActorCollection::Followers.of(&actor_id);
    let following = LocalActorCollection::Following.of(&actor_id);
    let subscribers = LocalActorCollection::Subscribers.of(&actor_id);

    let private_key = deserialize_private_key(&user.private_key)?;
    let public_key_pem = get_public_key_pem(&private_key)?;
    let public_key = PublicKey {
        id: format!("{}{}", actor_id, ACTOR_KEY_SUFFIX),
        owner: actor_id.clone(),
        public_key_pem: public_key_pem,
    };
    let avatar = match &user.profile.avatar_file_name {
        Some(file_name) => {
            let image = Image {
                object_type: IMAGE.to_string(),
                url: get_file_url(instance_url, file_name),
            };
            Some(image)
        },
        None => None,
    };
    let banner = match &user.profile.banner_file_name {
        Some(file_name) => {
            let image = Image {
                object_type: IMAGE.to_string(),
                url: get_file_url(instance_url, file_name),
            };
            Some(image)
        },
        None => None,
    };
    let mut attachments = vec![];
    for proof in user.profile.identity_proofs.clone().into_inner() {
        let attachment = ActorAttachment {
            object_type: IDENTITY_PROOF.to_string(),
            name: proof.issuer.to_string(),
            value: None,
            signature_algorithm: Some(proof.proof_type),
            signature_value: Some(proof.value),
        };
        attachments.push(attachment);
    };
    for field in user.profile.extra_fields.clone().into_inner() {
        let attachment = ActorAttachment {
            object_type: PROPERTY_VALUE.to_string(),
            name: field.name,
            value: Some(field.value),
            signature_algorithm: None,
            signature_value: None,
        };
        attachments.push(attachment);
    };
    let actor = Actor {
        context: Some(json!([
            AP_CONTEXT.to_string(),
            W3ID_CONTEXT.to_string(),
        ])),
        id: actor_id.clone(),
        object_type: PERSON.to_string(),
        name: user.profile.display_name.clone(),
        preferred_username: username.to_string(),
        inbox,
        outbox,
        followers: Some(followers),
        following: Some(following),
        subscribers: Some(subscribers),
        public_key,
        icon: avatar,
        image: banner,
        summary: user.profile.bio.clone(),
        attachment: Some(attachments),
        url: Some(actor_id),
    };
    Ok(actor)
}

pub fn get_instance_actor(
    instance: &Instance,
) -> Result<Actor, ActorKeyError> {
    let actor_id = instance.actor_id();
    let actor_inbox = LocalActorCollection::Inbox.of(&actor_id);
    let actor_outbox = LocalActorCollection::Outbox.of(&actor_id);
    let public_key_pem = get_public_key_pem(&instance.actor_key)?;
    let public_key = PublicKey {
        id: instance.actor_key_id(),
        owner: actor_id.clone(),
        public_key_pem: public_key_pem,
    };
    let actor = Actor {
        context: Some(json!([
            AP_CONTEXT.to_string(),
            W3ID_CONTEXT.to_string(),
        ])),
        id: actor_id,
        object_type: SERVICE.to_string(),
        name: Some(instance.host()),
        preferred_username: instance.host(),
        inbox: actor_inbox,
        outbox: actor_outbox,
        followers: None,
        following: None,
        subscribers: None,
        public_key,
        icon: None,
        image: None,
        summary: None,
        attachment: None,
        url: None,
    };
    Ok(actor)
}

#[cfg(test)]
mod tests {
    use url::Url;
    use crate::models::profiles::types::DbActorProfile;
    use crate::utils::crypto::{
        generate_weak_private_key,
        serialize_private_key,
    };
    use super::*;

    const INSTANCE_HOST: &str = "example.com";
    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_get_actor_address() {
        let actor = Actor {
            id: "https://test.org/users/1".to_string(),
            preferred_username: "test".to_string(),
            ..Default::default()
        };
        let actor_address = actor.address(INSTANCE_HOST).unwrap();
        assert_eq!(actor_address.is_local, false);
        assert_eq!(actor_address.acct(), "test@test.org");
    }

    #[test]
    fn test_local_actor() {
        let private_key = generate_weak_private_key().unwrap();
        let private_key_pem = serialize_private_key(&private_key).unwrap();
        let profile = DbActorProfile {
            username: "testuser".to_string(),
            bio: Some("testbio".to_string()),
            ..Default::default()
        };
        let user = User {
            private_key: private_key_pem,
            profile,
            ..Default::default()
        };
        let actor = get_local_actor(&user, INSTANCE_URL).unwrap();
        assert_eq!(actor.id, "https://example.com/users/testuser");
        assert_eq!(actor.preferred_username, user.profile.username);
        assert_eq!(actor.inbox, "https://example.com/users/testuser/inbox");
        assert_eq!(actor.outbox, "https://example.com/users/testuser/outbox");
        assert_eq!(
            actor.followers.unwrap(),
            "https://example.com/users/testuser/followers",
        );
        assert_eq!(
            actor.following.unwrap(),
            "https://example.com/users/testuser/following",
        );
        assert_eq!(
            actor.subscribers.unwrap(),
            "https://example.com/users/testuser/subscribers",
        );
        assert_eq!(
            actor.public_key.id,
            "https://example.com/users/testuser#main-key",
        );
        assert_eq!(actor.attachment.unwrap().len(), 0);
        assert_eq!(actor.summary, user.profile.bio);
    }

    #[test]
    fn test_instance_actor() {
        let instance_url = Url::parse("https://example.com/").unwrap();
        let instance_rsa_key = generate_weak_private_key().unwrap();
        let instance = Instance::new(instance_url, instance_rsa_key);
        let actor = get_instance_actor(&instance).unwrap();
        assert_eq!(actor.id, "https://example.com/actor");
        assert_eq!(actor.object_type, "Service");
        assert_eq!(actor.preferred_username, "example.com");
        assert_eq!(actor.inbox, "https://example.com/actor/inbox");
        assert_eq!(actor.outbox, "https://example.com/actor/outbox");
        assert_eq!(actor.public_key.id, "https://example.com/actor#main-key");
    }
}
