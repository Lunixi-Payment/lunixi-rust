use std::fmt;

/// A safe SDK error. Upstream bodies, credentials and URLs are never retained.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// Configuration/request validation failed before network access.
    Invalid(&'static str),
    /// HTTP/API failure with safe diagnostics only.
    Api {
        status: u16,
        code: String,
        request_id: Option<String>,
    },
    /// Transport failure; underlying errors may contain URLs and are omitted.
    Transport,
    /// The bounded response could not be read or decoded.
    Response(&'static str),
    /// A webhook was invalid, stale, tampered with or collided with an event ID.
    Webhook,
    /// Durable storage failed. Do not acknowledge this webhook with HTTP 2xx.
    Inbox,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(field) => write!(f, "lunixi: invalid {field}"),
            Self::Api { status, code, .. } => {
                write!(f, "lunixi: API request failed (HTTP {status}, code {code})")
            }
            Self::Transport => f.write_str("lunixi: transport failed"),
            Self::Response(reason) => write!(f, "lunixi: invalid response ({reason})"),
            Self::Webhook => f.write_str("lunixi: webhook verification failed"),
            Self::Inbox => f.write_str("lunixi: webhook persistence failed"),
        }
    }
}
impl std::error::Error for Error {}
/// SDK result type.
pub type Result<T> = std::result::Result<T, Error>;
