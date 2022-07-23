use crate::activitypub::vocabulary::{IDENTITY_PROOF, PROPERTY_VALUE};
use crate::errors::ValidationError;
use crate::ethereum::identity::{
    ETHEREUM_EIP191_PROOF,
    DidPkh,
    verify_identity_proof,
};
use crate::models::profiles::types::{ExtraField, IdentityProof};
use super::types::ActorAttachment;

pub fn attach_identity_proof(
    proof: IdentityProof,
) -> ActorAttachment {
    ActorAttachment {
        object_type: IDENTITY_PROOF.to_string(),
        name: proof.issuer.to_string(),
        value: None,
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

pub fn attach_extra_field(
    field: ExtraField,
) -> ActorAttachment {
    ActorAttachment {
        object_type: PROPERTY_VALUE.to_string(),
        name: field.name,
        value: Some(field.value),
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
    use super::*;

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
}
