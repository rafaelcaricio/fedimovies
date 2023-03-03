use serde::{
    Deserialize,
    Deserializer,
    de::Error as DeserializerError,
};

#[derive(Clone, PartialEq)]
pub enum RegistrationType {
    Open,
    Invite,
}

impl Default for RegistrationType {
    fn default() -> Self { Self::Invite }
}

impl<'de> Deserialize<'de> for RegistrationType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        let registration_type_str = String::deserialize(deserializer)?;
        let registration_type = match registration_type_str.as_str() {
            "open" => Self::Open,
            "invite" => Self::Invite,
            _ => return Err(DeserializerError::custom("unknown registration type")),
        };
        Ok(registration_type)
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct RegistrationConfig {
    #[serde(rename = "type")]
    pub registration_type: RegistrationType,

    #[serde(default)]
    pub default_role_read_only_user: bool, // default is false
}
