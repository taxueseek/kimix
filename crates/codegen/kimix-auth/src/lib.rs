//! Auth dependency-inversion seam shared between `Kimix-file-utils`
//! (the holder) and `Kimix-shell` (the implementer). Keeps shell types
//! out of data-collector's import graph while still letting refresh-aware
//! token resolution drive HTTP requests.
pub mod auth_provider;
#[cfg(feature = "middleware")]
pub mod retry_middleware;
#[cfg(test)]
mod tests;
pub mod visibility;

pub use auth_provider::{AuthCredentialProvider, CredentialSnapshot, StaticAuthCredentialProvider};
#[cfg(feature = "middleware")]
pub use retry_middleware::AuthRetryMiddleware;
pub use visibility::HttpAuth;
