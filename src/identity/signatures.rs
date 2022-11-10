/// Signature suites

// Identity proof, version 00
pub const PROOF_TYPE_ID_EIP191: &str = "ethereum-eip191-00";

// Identity proof, version 2022A
pub const PROOF_TYPE_ID_MINISIGN: &str = "MitraMinisignSignature2022A";

// Similar to https://identity.foundation/JcsEd25519Signature2020/
// - Canonicalization algorithm: JCS
// - Digest algorithm: SHA-256
// - Signature algorithm: RSASSA-PKCS1-v1_5
pub const PROOF_TYPE_JCS_RSA: &str = "JcsRsaSignature2022";

// Similar to EthereumPersonalSignature2021 but with JCS
pub const PROOF_TYPE_JCS_EIP191: &str ="JcsEip191Signature2022";

// Version 2022A
pub const PROOF_TYPE_JCS_MINISIGN: &str = "MitraJcsMinisignSignature2022A";
