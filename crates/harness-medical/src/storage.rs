// IMPLEMENTS: D-363
//! Medical mode storage requirements. The `harness-storage` /
//! SQLCipher integration (D-211) is the actual implementation; this
//! module is the pure-data spec medical mode hands to a session
//! initialiser so it knows to refuse plaintext-at-rest.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MedicalStorageRequirements {
    pub at_rest_encryption: bool,
    pub key_in_os_keyring: bool,
    pub refuse_plaintext_swap: bool,
}

#[must_use]
pub fn requirements() -> MedicalStorageRequirements {
    MedicalStorageRequirements {
        at_rest_encryption: true,
        key_in_os_keyring: true,
        refuse_plaintext_swap: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medical_requirements_are_strict_by_default() {
        let r = requirements();
        assert!(r.at_rest_encryption);
        assert!(r.key_in_os_keyring);
        assert!(r.refuse_plaintext_swap);
    }
}
