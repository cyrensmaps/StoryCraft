use std::fmt;

use serde::{Deserialize, Serialize};

/// A stable identifier for any addressable item in a project.
///
/// Newly created items get a random UUID; imported items keep whatever
/// identifier their source format used, so the inner value is free-form.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id(String);

impl Id {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new() -> Self {
        Id(uuid::Uuid::new_v4().simple().to_string())
    }

    /// The WebAssembly player never authors content, so a process-unique
    /// counter is enough and avoids needing a randomness source.
    #[cfg(target_arch = "wasm32")]
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Id(format!("rt{}", NEXT.fetch_add(1, Ordering::Relaxed)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for Id {
    fn default() -> Self {
        Id::new()
    }
}

impl From<&str> for Id {
    fn from(s: &str) -> Self {
        Id(s.to_owned())
    }
}

impl From<String> for Id {
    fn from(s: String) -> Self {
        Id(s)
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({})", self.0)
    }
}
