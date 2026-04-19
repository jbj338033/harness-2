use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! newtype_id {
    ($name:ident, $prefix:literal) => {
        #[doc = concat!("A typed identifier for a ", stringify!($name), ".")]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            #[must_use]
            pub fn as_uuid(&self) -> Uuid {
                self.0
            }

            /// # Errors
            /// Returns a [`uuid::Error`] if the body is not a valid UUID.
            pub fn parse(s: &str) -> Result<Self, uuid::Error> {
                let body = s.strip_prefix(concat!($prefix, "-")).unwrap_or(s);
                Ok(Self(Uuid::parse_str(body)?))
            }
        }

        impl std::str::FromStr for $name {
            type Err = uuid::Error;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::parse(s)
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}-{}", $prefix, self.0)
            }
        }

        impl From<Uuid> for $name {
            fn from(u: Uuid) -> Self {
                Self(u)
            }
        }

        impl From<$name> for Uuid {
            fn from(id: $name) -> Uuid {
                id.0
            }
        }
    };
}

newtype_id!(SessionId, "sess");
newtype_id!(AgentId, "agt");
newtype_id!(MessageId, "msg");
newtype_id!(ToolCallId, "tc");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let a = SessionId::new();
        let b = SessionId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn display_has_prefix() {
        let id = AgentId::new();
        assert!(format!("{id}").starts_with("agt-"));
    }

    #[test]
    fn serde_roundtrip() {
        let id = MessageId::new();
        let json = serde_json::to_string(&id).unwrap();
        let back: MessageId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }
}
