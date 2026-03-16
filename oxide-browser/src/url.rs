//! WHATWG URL Standard compliant URL parsing for the Oxide browser.
//!
//! Wraps the `url` crate (which implements the WHATWG URL spec) and adds
//! Oxide-specific scheme handling (`oxide://` for internal pages) alongside
//! standard `http`, `https`, and `file` schemes.

use std::fmt;

use url::Url;

const SUPPORTED_SCHEMES: &[&str] = &["http", "https", "file", "oxide"];

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OxideUrl {
    inner: Url,
}

#[derive(Debug)]
pub enum UrlError {
    Parse(String),
    UnsupportedScheme(String),
    Empty,
    RelativeRequiresBase,
}

impl fmt::Display for UrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UrlError::Parse(msg) => write!(f, "URL parse error: {msg}"),
            UrlError::UnsupportedScheme(s) => write!(f, "unsupported URL scheme: {s}"),
            UrlError::Empty => write!(f, "empty URL"),
            UrlError::RelativeRequiresBase => {
                write!(f, "relative URL cannot be parsed without a base URL")
            }
        }
    }
}

impl std::error::Error for UrlError {}

impl OxideUrl {
    /// Parse a user-supplied URL string.
    ///
    /// Bare hostnames like `example.com/path` are assumed HTTPS.
    /// Relative paths (starting with `/` or `.`) are rejected — use
    /// [`OxideUrl::join`] to resolve them against a base URL.
    pub fn parse(input: &str) -> Result<Self, UrlError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(UrlError::Empty);
        }

        if (trimmed.starts_with('/') || trimmed.starts_with('.')) && !trimmed.starts_with("//") {
            return Err(UrlError::RelativeRequiresBase);
        }

        let normalized = if trimmed.contains("://") || trimmed.starts_with("//") {
            trimmed.to_string()
        } else {
            format!("https://{trimmed}")
        };

        let inner = Url::parse(&normalized).map_err(|e| UrlError::Parse(e.to_string()))?;

        if !SUPPORTED_SCHEMES.contains(&inner.scheme()) {
            return Err(UrlError::UnsupportedScheme(inner.scheme().to_string()));
        }

        Ok(Self { inner })
    }

    /// Resolve a possibly-relative reference against this URL as the base.
    pub fn join(&self, reference: &str) -> Result<Self, UrlError> {
        let inner = self
            .inner
            .join(reference)
            .map_err(|e| UrlError::Parse(e.to_string()))?;

        if !SUPPORTED_SCHEMES.contains(&inner.scheme()) {
            return Err(UrlError::UnsupportedScheme(inner.scheme().to_string()));
        }

        Ok(Self { inner })
    }

    pub fn scheme(&self) -> &str {
        self.inner.scheme()
    }

    pub fn host_str(&self) -> Option<&str> {
        self.inner.host_str()
    }

    pub fn port(&self) -> Option<u16> {
        self.inner.port()
    }

    pub fn path(&self) -> &str {
        self.inner.path()
    }

    pub fn query(&self) -> Option<&str> {
        self.inner.query()
    }

    pub fn fragment(&self) -> Option<&str> {
        self.inner.fragment()
    }

    pub fn as_str(&self) -> &str {
        self.inner.as_str()
    }

    /// True for http/https URLs that can be fetched over the network.
    pub fn is_fetchable(&self) -> bool {
        matches!(self.scheme(), "http" | "https")
    }

    /// True for `file://` URLs.
    pub fn is_local_file(&self) -> bool {
        self.scheme() == "file"
    }

    /// True for `oxide://` internal browser pages.
    pub fn is_internal(&self) -> bool {
        self.scheme() == "oxide"
    }

    /// Extract the local filesystem path from a `file://` URL.
    pub fn to_file_path(&self) -> Option<std::path::PathBuf> {
        self.inner.to_file_path().ok()
    }

    pub fn set_fragment(&mut self, fragment: Option<&str>) {
        self.inner.set_fragment(fragment);
    }

    pub fn set_query(&mut self, query: Option<&str>) {
        self.inner.set_query(query);
    }

    pub fn query_pairs(&self) -> Vec<(String, String)> {
        self.inner
            .query_pairs()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    /// Scheme + host + port serialized as a string (for same-origin checks).
    pub fn origin_str(&self) -> String {
        match self.inner.origin() {
            url::Origin::Opaque(_) => self.scheme().to_string(),
            url::Origin::Tuple(scheme, host, port) => {
                format!("{scheme}://{host}:{port}")
            }
        }
    }

    /// Check whether two URLs share the same origin.
    pub fn same_origin(&self, other: &OxideUrl) -> bool {
        self.inner.origin() == other.inner.origin()
    }
}

impl fmt::Display for OxideUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

/// Percent-encode a string (useful for building URL path/query components).
pub fn percent_encode(input: &str) -> String {
    percent_encoding::utf8_percent_encode(input, percent_encoding::NON_ALPHANUMERIC).to_string()
}

/// Decode a percent-encoded string.
pub fn percent_decode(input: &str) -> String {
    percent_encoding::percent_decode_str(input)
        .decode_utf8_lossy()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_https() {
        let url = OxideUrl::parse("https://example.com/app.wasm").unwrap();
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("example.com"));
        assert_eq!(url.path(), "/app.wasm");
    }

    #[test]
    fn bare_hostname_becomes_https() {
        let url = OxideUrl::parse("example.com/app.wasm").unwrap();
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.as_str(), "https://example.com/app.wasm");
    }

    #[test]
    fn resolve_relative() {
        let base = OxideUrl::parse("https://example.com/apps/v1/main.wasm").unwrap();
        let resolved = base.join("../v2/new.wasm").unwrap();
        assert_eq!(resolved.as_str(), "https://example.com/apps/v2/new.wasm");
    }

    #[test]
    fn file_url() {
        let url = OxideUrl::parse("file:///tmp/app.wasm").unwrap();
        assert!(url.is_local_file());
        assert!(!url.is_fetchable());
    }

    #[test]
    fn oxide_internal() {
        let url = OxideUrl::parse("oxide://home").unwrap();
        assert!(url.is_internal());
    }

    #[test]
    fn unsupported_scheme() {
        assert!(OxideUrl::parse("ftp://example.com").is_err());
    }

    #[test]
    fn query_and_fragment() {
        let url = OxideUrl::parse("https://example.com/app.wasm?v=1#section").unwrap();
        assert_eq!(url.query(), Some("v=1"));
        assert_eq!(url.fragment(), Some("section"));
    }

    #[test]
    fn percent_encoding_roundtrip() {
        let original = "hello world";
        let encoded = percent_encode(original);
        let decoded = percent_decode(&encoded);
        assert_eq!(decoded, original);
    }

    #[test]
    fn relative_path_rejected_without_base() {
        assert!(matches!(
            OxideUrl::parse("../other.wasm"),
            Err(UrlError::RelativeRequiresBase)
        ));
    }
}
