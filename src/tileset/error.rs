use std::fmt;
use std::io::ErrorKind;

/// How much of an upstream error body is kept in the error message. Object
/// storage answers with XML/HTML that is useful to see but not worth logging
/// in full.
const MAX_BODY_EXCERPT: usize = 256;

/// Error returned by a remote tile backend.
///
/// Keeping the upstream status code instead of collapsing everything into a
/// string is what lets the service tell a genuinely missing tile apart from a
/// degraded object store, both for retry decisions and for the status code the
/// client ends up seeing.
#[derive(Debug)]
pub enum TileError {
    /// Upstream answered, but the tile does not exist. `detail` says how that
    /// was established (`HTTP 404`, `no such file`, ...).
    NotFound { key: String, detail: String },
    /// Upstream answered with a non-success status other than 404.
    Upstream {
        key: String,
        status: u16,
        body: String,
    },
    /// The request never completed (connection reset, timeout, TLS, DNS, ...).
    Transport { key: String, message: String },
    /// Bytes arrived but could not be decoded.
    Decode { key: String, message: String },
}

impl TileError {
    pub fn upstream(key: impl Into<String>, status: u16, body: impl Into<String>) -> Self {
        let key = key.into();
        if status == 404 {
            TileError::NotFound {
                key,
                detail: "HTTP 404".to_string(),
            }
        } else {
            TileError::Upstream {
                key,
                status,
                body: excerpt(body.into()),
            }
        }
    }

    pub fn not_found(key: impl Into<String>, detail: impl fmt::Display) -> Self {
        TileError::NotFound {
            key: key.into(),
            detail: detail.to_string(),
        }
    }

    pub fn transport(key: impl Into<String>, message: impl fmt::Display) -> Self {
        TileError::Transport {
            key: key.into(),
            message: message.to_string(),
        }
    }

    pub fn decode(key: impl Into<String>, message: impl fmt::Display) -> Self {
        TileError::Decode {
            key: key.into(),
            message: message.to_string(),
        }
    }

    /// Whether another attempt has a realistic chance of succeeding.
    ///
    /// Transient object-storage failures (Hetzner S3 in particular) show up as
    /// 5xx, 429 or a dropped connection; a 404 or a malformed tile will not get
    /// better by asking again.
    pub fn is_retryable(&self) -> bool {
        match self {
            TileError::Transport { .. } => true,
            TileError::Upstream { status, .. } => {
                *status >= 500 || *status == 408 || *status == 429
            }
            TileError::NotFound { .. } | TileError::Decode { .. } => false,
        }
    }

    /// The `io::ErrorKind` this maps to, which decides the HTTP status the
    /// caller receives: a missing tile is a 404, a degraded upstream a 500.
    pub fn error_kind(&self) -> ErrorKind {
        match self {
            TileError::NotFound { .. } => ErrorKind::NotFound,
            _ => ErrorKind::Other,
        }
    }
}

impl fmt::Display for TileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TileError::NotFound { key, detail } => {
                write!(f, "tile {} not found ({})", key, detail)
            }
            TileError::Upstream { key, status, body } => {
                write!(f, "GET {} returned HTTP {}: {}", key, status, body)
            }
            TileError::Transport { key, message } => {
                write!(f, "request for {} failed: {}", key, message)
            }
            TileError::Decode { key, message } => {
                write!(f, "failed to decode tile {}: {}", key, message)
            }
        }
    }
}

impl std::error::Error for TileError {}

impl From<TileError> for std::io::Error {
    fn from(err: TileError) -> Self {
        std::io::Error::new(err.error_kind(), err.to_string())
    }
}

fn excerpt(body: String) -> String {
    let trimmed = body.trim();
    if trimmed.len() <= MAX_BODY_EXCERPT {
        return trimmed.to_string();
    }
    // Cut on a char boundary so multi-byte bodies cannot panic here.
    let mut end = MAX_BODY_EXCERPT;
    while end > 0 && !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… (truncated)", &trimmed[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_404_to_not_found() {
        let err = TileError::upstream("N45/N45E009.hgt.gz", 404, "<Error>NoSuchKey</Error>");
        assert!(matches!(err, TileError::NotFound { .. }));
        assert!(!err.is_retryable());
        assert_eq!(err.error_kind(), ErrorKind::NotFound);
    }

    #[test]
    fn server_errors_are_retryable_and_not_found_errors() {
        for status in [500, 502, 503, 504, 429, 408] {
            let err = TileError::upstream("key", status, "boom");
            assert!(err.is_retryable(), "HTTP {} should be retryable", status);
            assert_eq!(err.error_kind(), ErrorKind::Other);
        }
    }

    #[test]
    fn client_errors_other_than_timeouts_are_not_retryable() {
        for status in [400, 401, 403] {
            assert!(!TileError::upstream("key", status, "nope").is_retryable());
        }
    }

    #[test]
    fn decode_errors_are_not_retryable() {
        let err = TileError::decode("key", "invalid gzip header");
        assert!(!err.is_retryable());
        assert!(err.to_string().contains("invalid gzip header"));
    }

    #[test]
    fn transport_errors_are_retryable() {
        assert!(TileError::transport("key", "connection reset").is_retryable());
    }

    #[test]
    fn upstream_message_keeps_status_code() {
        let err = TileError::upstream("N50/N50E002.hgt.gz", 504, "<Error>Timeout</Error>");
        let msg = err.to_string();
        assert!(msg.contains("504"), "{}", msg);
        assert!(msg.contains("N50/N50E002.hgt.gz"), "{}", msg);
    }

    #[test]
    fn long_bodies_are_truncated_on_char_boundaries() {
        let body = "ä".repeat(1000);
        let err = TileError::upstream("key", 500, body);
        // Reaching Display at all proves no mid-char slice happened.
        assert!(err.to_string().contains("truncated"));
    }
}
