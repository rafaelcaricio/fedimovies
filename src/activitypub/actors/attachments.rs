use fedimovies_models::profiles::types::{
    ExtraField, IdentityProof, IdentityProofType, PaymentLink, PaymentOption,
};

use crate::activitypub::vocabulary::{IDENTITY_PROOF, LINK, PROPERTY_VALUE};
use crate::errors::ValidationError;
use crate::json_signatures::proofs::{PROOF_TYPE_ID_EIP191, PROOF_TYPE_ID_MINISIGN};
use crate::web_client::urls::get_subscription_page_url;

use super::types::ActorAttachment;

pub fn attach_identity_proof(proof: IdentityProof) -> ActorAttachment {
    let proof_type_str = match proof.proof_type {
        IdentityProofType::LegacyEip191IdentityProof => PROOF_TYPE_ID_EIP191,
        IdentityProofType::LegacyMinisignIdentityProof => PROOF_TYPE_ID_MINISIGN,
    };
    ActorAttachment {
        object_type: IDENTITY_PROOF.to_string(),
        name: proof.issuer.to_string(),
        value: None,
        href: None,
        signature_algorithm: Some(proof_type_str.to_string()),
        signature_value: Some(proof.value),
    }
}

pub fn parse_identity_proof(
    _actor_id: &str,
    _attachment: &ActorAttachment,
) -> Result<IdentityProof, ValidationError> {
    Err(ValidationError("incorrect proof type".to_string()))
}

pub fn attach_payment_option(
    instance_url: &str,
    username: &str,
    payment_option: PaymentOption,
) -> ActorAttachment {
    let (name, href) = match payment_option {
        // Local actors can't have payment links
        PaymentOption::Link(_) => unimplemented!(),
        PaymentOption::EthereumSubscription(_) => {
            let name = "EthereumSubscription".to_string();
            let href = get_subscription_page_url(instance_url, username);
            (name, href)
        }
        PaymentOption::MoneroSubscription(_) => {
            let name = "MoneroSubscription".to_string();
            let href = get_subscription_page_url(instance_url, username);
            (name, href)
        }
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
        return Err(ValidationError("invalid attachment type".to_string()));
    };
    let href = attachment
        .href
        .as_ref()
        .ok_or(ValidationError("href attribute is required".to_string()))?
        .to_string();
    let payment_option = PaymentOption::Link(PaymentLink {
        name: attachment.name.clone(),
        href: href,
    });
    Ok(payment_option)
}

pub fn attach_extra_field(field: ExtraField) -> ActorAttachment {
    ActorAttachment {
        object_type: PROPERTY_VALUE.to_string(),
        name: field.name,
        value: Some(field.value),
        href: None,
        signature_algorithm: None,
        signature_value: None,
    }
}

pub fn parse_extra_field(attachment: &ActorAttachment) -> Result<ExtraField, ValidationError> {
    if attachment.object_type != PROPERTY_VALUE {
        return Err(ValidationError("invalid attachment type".to_string()));
    };
    let property_value = attachment
        .value
        .as_ref()
        .ok_or(ValidationError("missing property value".to_string()))?;
    let field = ExtraField {
        name: attachment.name.clone(),
        value: property_value.to_string(),
        value_source: None,
    };
    Ok(field)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fedimovies_utils::caip2::ChainId;

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
        let username = "testuser";
        let payment_option = PaymentOption::ethereum_subscription(ChainId::ethereum_mainnet());
        let subscription_page_url = "https://example.com/@testuser/subscription";
        let attachment = attach_payment_option(INSTANCE_URL, username, payment_option);
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
