use uuid::Uuid;

use crate::activitypub::vocabulary::{
    IDENTITY_PROOF,
    LINK,
    PROPERTY_VALUE,
};
use crate::errors::ValidationError;
use crate::ethereum::identity::{
    ETHEREUM_EIP191_PROOF,
    verify_eip191_identity_proof,
};
use crate::frontend::get_subscription_page_url;
use crate::identity::did::Did;
use crate::models::profiles::types::{
    ExtraField,
    IdentityProof,
    PaymentLink,
    PaymentOption,
};
use super::types::ActorAttachment;

pub fn attach_identity_proof(
    proof: IdentityProof,
) -> ActorAttachment {
    ActorAttachment {
        object_type: IDENTITY_PROOF.to_string(),
        name: proof.issuer.to_string(),
        value: None,
        href: None,
        signature_algorithm: Some(proof.proof_type),
        signature_value: Some(proof.value),
    }
}

pub fn parse_identity_proof(
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
    let did = attachment.name.parse::<Did>()
        .map_err(|_| ValidationError("invalid did"))?;
    let Did::Pkh(ref did_pkh) = did;
    let signature = attachment.signature_value.as_ref()
        .ok_or(ValidationError("missing signature"))?;
    verify_eip191_identity_proof(
        actor_id,
        did_pkh,
        signature,
    ).map_err(|_| ValidationError("invalid identity proof"))?;
    let proof = IdentityProof {
        issuer: did,
        proof_type: proof_type.to_string(),
        value: signature.to_string(),
    };
    Ok(proof)
}

pub fn attach_payment_option(
    instance_url: &str,
    user_id: &Uuid,
    payment_option: PaymentOption,
) -> ActorAttachment {
    let (name, href) = match payment_option {
        // Local actors can't have payment links
        PaymentOption::Link(_) => unimplemented!(),
        PaymentOption::EthereumSubscription(_) => {
            let name = "EthereumSubscription".to_string();
            let href = get_subscription_page_url(instance_url, user_id);
            (name, href)
        },
        PaymentOption::MoneroSubscription(_) => {
            let name = "MoneroSubscription".to_string();
            let href = get_subscription_page_url(instance_url, user_id);
            (name, href)
        },
    };
    ActorAttachment {
        object_type: LINK.to_string(),
        name: name,
        value: None,
        href: Some(href),
        signature_algorithm: None,
        signature_value: None,
    }
}

pub fn parse_payment_option(
    attachment: &ActorAttachment,
) -> Result<PaymentOption, ValidationError> {
    if attachment.object_type != LINK {
        return Err(ValidationError("invalid attachment type"));
    };
    let href = attachment.href.as_ref()
        .ok_or(ValidationError("href attribute is required"))?
        .to_string();
    let payment_option = PaymentOption::Link(PaymentLink {
        name: attachment.name.clone(),
        href: href,
    });
    Ok(payment_option)
}

pub fn attach_extra_field(
    field: ExtraField,
) -> ActorAttachment {
    ActorAttachment {
        object_type: PROPERTY_VALUE.to_string(),
        name: field.name,
        value: Some(field.value),
        href: None,
        signature_algorithm: None,
        signature_value: None,
    }
}

pub fn parse_extra_field(
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

#[cfg(test)]
mod tests {
    use crate::utils::caip2::ChainId;
    use crate::utils::id::new_uuid;
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_extra_field() {
        let field = ExtraField {
            name: "test".to_string(),
            value: "value".to_string(),
            value_source: None,
        };
        let attachment = attach_extra_field(field.clone());
        assert_eq!(attachment.object_type, PROPERTY_VALUE);

        let parsed_field = parse_extra_field(&attachment).unwrap();
        assert_eq!(parsed_field.name, field.name);
        assert_eq!(parsed_field.value, field.value);
    }

    #[test]
    fn test_payment_option() {
        let user_id = new_uuid();
        let payment_option =
            PaymentOption::ethereum_subscription(ChainId::ethereum_mainnet());
        let subscription_page_url =
            format!("https://example.com/profile/{}/subscription", user_id);
        let attachment = attach_payment_option(
            INSTANCE_URL,
            &user_id,
            payment_option,
        );
        assert_eq!(attachment.object_type, LINK);
        assert_eq!(attachment.name, "EthereumSubscription");
        assert_eq!(attachment.href.as_deref().unwrap(), subscription_page_url);

        let parsed_option = parse_payment_option(&attachment).unwrap();
        let link = match parsed_option {
            PaymentOption::Link(link) => link,
            _ => panic!("wrong option"),
        };
        assert_eq!(link.name, "EthereumSubscription");
        assert_eq!(link.href, subscription_page_url);
    }
}
