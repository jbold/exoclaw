use std::fmt;

/// Parsed capability grant for a plugin.
///
/// Capabilities are specified in config as strings like `"http:api.example.com"`
/// and parsed into this enum. Each variant maps to specific Extism Manifest settings.
#[derive(Debug, Clone, PartialEq)]
pub enum Capability {
    /// HTTP access to a specific host (e.g., "api.telegram.org").
    Http(String),
    /// Host storage access (e.g., "sessions").
    Store(String),
    /// Named host function access.
    HostFunction(String),
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(host) => write!(f, "http:{host}"),
            Self::Store(name) => write!(f, "store:{name}"),
            Self::HostFunction(name) => write!(f, "host_function:{name}"),
        }
    }
}

/// Parse a capability string from config into a typed `Capability`.
///
/// Format: `"type:value"` where type is one of: `http`, `store`, `host_function`.
pub fn parse(s: &str) -> anyhow::Result<Capability> {
    let (cap_type, value) = s.split_once(':').ok_or_else(|| {
        anyhow::anyhow!(
            "invalid capability format '{s}': expected 'type:value' (e.g., 'http:api.example.com')"
        )
    })?;

    if value.is_empty() {
        anyhow::bail!("capability value cannot be empty in '{s}'");
    }

    match cap_type {
        "http" => Ok(Capability::Http(value.to_string())),
        "store" => Ok(Capability::Store(value.to_string())),
        "host_function" => Ok(Capability::HostFunction(value.to_string())),
        _ => anyhow::bail!(
            "unknown capability type '{cap_type}' in '{s}': expected 'http', 'store', or 'host_function'"
        ),
    }
}

/// Parse a list of capability strings, returning all or failing on first invalid.
pub fn parse_all(caps: &[String]) -> anyhow::Result<Vec<Capability>> {
    caps.iter().map(|s| parse(s)).collect()
}

/// Extract `allowed_hosts` from a list of capabilities (for Extism Manifest).
pub fn allowed_hosts(caps: &[Capability]) -> Vec<String> {
    caps.iter()
        .filter_map(|c| match c {
            Capability::Http(host) => Some(host.clone()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_http_capability() {
        let cap = parse("http:api.telegram.org").unwrap();
        assert_eq!(cap, Capability::Http("api.telegram.org".into()));
    }

    #[test]
    fn parse_store_capability() {
        let cap = parse("store:sessions").unwrap();
        assert_eq!(cap, Capability::Store("sessions".into()));
    }

    #[test]
    fn parse_host_function_capability() {
        let cap = parse("host_function:my_func").unwrap();
        assert_eq!(cap, Capability::HostFunction("my_func".into()));
    }

    #[test]
    fn parse_unknown_type_fails() {
        let result = parse("filesystem:tmp");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("unknown capability type")
        );
    }

    #[test]
    fn parse_missing_colon_fails() {
        let result = parse("http");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("expected 'type:value'")
        );
    }

    #[test]
    fn parse_empty_value_fails() {
        let result = parse("http:");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn parse_all_succeeds() {
        let caps = parse_all(&["http:api.example.com".into(), "store:data".into()]).unwrap();
        assert_eq!(caps.len(), 2);
    }

    #[test]
    fn parse_all_fails_on_invalid() {
        let result = parse_all(&["http:api.example.com".into(), "bad".into()]);
        assert!(result.is_err());
    }

    #[test]
    fn allowed_hosts_filters_http() {
        let caps = vec![
            Capability::Http("api.example.com".into()),
            Capability::Store("sessions".into()),
            Capability::Http("api.other.com".into()),
        ];
        let hosts = allowed_hosts(&caps);
        assert_eq!(hosts, vec!["api.example.com", "api.other.com"]);
    }

    #[test]
    fn display_capability() {
        assert_eq!(Capability::Http("host".into()).to_string(), "http:host");
        assert_eq!(Capability::Store("s".into()).to_string(), "store:s");
        assert_eq!(
            Capability::HostFunction("f".into()).to_string(),
            "host_function:f"
        );
    }
}
