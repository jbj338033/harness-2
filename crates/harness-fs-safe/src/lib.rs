// IMPLEMENTS: D-067, D-113, D-128
//! FS hardening primitives.
//!
//! - [`can_write`] — D-067: canonicalize-then-check + symlink
//!   resolve gate. The pure-data verdict here mirrors what the
//!   tools layer must enforce on every write.
//! - [`openat2_spec`] — D-113: per-platform `OpenatSafeFlags`
//!   spec — Linux `openat2(RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS)`,
//!   macOS `O_NOFOLLOW_ANY` + component check fallback. The actual
//!   syscall binding lives in `harness-tools-fs`; this module is
//!   the platform-neutral spec the tests check against.
//! - [`corrupted_root`] — D-128: five-signal corrupted-root
//!   detector. 4/5 signals → root accepted; below 4 →
//!   `ClarificationQuestion` so the daemon asks the operator.

pub mod can_write;
pub mod corrupted_root;
pub mod openat2_spec;

pub use can_write::{CanWriteVerdict, evaluate_can_write};
pub use corrupted_root::{CorruptedRootSignals, CorruptedRootVerdict, classify_root};
pub use openat2_spec::{OpenatSafeFlags, Platform, flags_for};
