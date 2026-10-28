use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

/// Cryptographically secure SHA-256 digest of an action, its payload, target, and scope.
///
/// Law: Approvals and capability grants are bound to an exact ActionDigest.
/// Changing the action payload or target invalidates the approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ActionDigest([u8; 32]);

impl ActionDigest {
    /// Computes a new ActionDigest by SHA-256 hashing the given byte slice.
    pub fn compute(payload: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(payload);
        Self(hasher.finalize().into())
    }

    /// Computes a digest from any serializable object using its canonical JSON representation.
    pub fn from_serializable<T: Serialize>(value: &T) -> Result<Self, serde_json::Error> {
        let bytes = serde_json::to_vec(value)?;
        Ok(Self::compute(&bytes))
    }

    /// Creates an ActionDigest from raw 32 bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the raw 32 bytes of the digest.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Returns a lowercase hex string representation.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

impl fmt::Display for ActionDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_digest_determinism() {
        let d1 =
            ActionDigest::compute(b"{\"action\":\"send_email\",\"to\":\"client@example.com\"}");
        let d2 =
            ActionDigest::compute(b"{\"action\":\"send_email\",\"to\":\"client@example.com\"}");
        let d3 = ActionDigest::compute(b"{\"action\":\"send_email\",\"to\":\"other@example.com\"}");

        assert_eq!(d1, d2);
        assert_ne!(d1, d3);
    }
}
