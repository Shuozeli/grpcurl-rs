use base64::Engine;
use regex::Regex;
use std::sync::LazyLock;
use tonic::metadata::{AsciiMetadataValue, MetadataMap};

use crate::error::{GrpcurlError, Result};

/// Regex for matching `${VAR_NAME}` patterns in header values.
///
/// Equivalent to Go's `envVarRegex = regexp.MustCompile(`\$\{\w+\}`)`.
static ENV_VAR_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$\{(\w+)\}").expect("env var regex is valid"));

/// Base64 engines for lenient binary header decoding.
///
/// Go tries 4 codecs in order: standard, URL-safe, raw-standard, raw-URL-safe.
/// We do the same for maximum compatibility.
static BASE64_ENGINES: LazyLock<Vec<(&str, base64::engine::GeneralPurpose)>> =
    LazyLock::new(|| {
        use base64::engine::general_purpose;
        vec![
            ("standard", general_purpose::STANDARD),
            ("url-safe", general_purpose::URL_SAFE),
            ("standard (no pad)", general_purpose::STANDARD_NO_PAD),
            ("url-safe (no pad)", general_purpose::URL_SAFE_NO_PAD),
        ]
    });

/// Parse header strings in `"Name: Value"` format into a gRPC MetadataMap.
///
/// Equivalent to Go's `MetadataFromHeaders()` (grpcurl.go).
///
/// Rules (matching Go behavior):
/// - Splits on the first `:` in each header string
/// - Header name is lowercased
/// - No colon means the value is empty
/// - Binary headers (name ending in `-bin`) have their value decoded from
///   base64, trying 4 codecs before falling back to the raw string
pub fn metadata_from_headers(headers: &[String]) -> MetadataMap {
    let mut map = MetadataMap::new();

    for header in headers {
        let (name, value) = match header.split_once(':') {
            Some((n, v)) => (n.trim().to_lowercase(), v.trim().to_string()),
            None => (header.trim().to_lowercase(), String::new()),
        };

        if name.is_empty() {
            continue;
        }

        if name.ends_with("-bin") {
            // Binary header: try base64 decode with multiple codecs
            match tonic::metadata::BinaryMetadataKey::from_bytes(name.as_bytes()) {
                Ok(key) => {
                    let bytes = try_base64_decode(&value).unwrap_or_else(|| value.into_bytes());
                    let val = tonic::metadata::BinaryMetadataValue::from_bytes(&bytes);
                    map.append_bin(key, val);
                }
                Err(_) => {
                    eprintln!("warning: header {header:?} dropped: invalid binary metadata key");
                }
            }
        } else {
            // ASCII header
            match value.parse::<AsciiMetadataValue>() {
                Ok(val) => match tonic::metadata::AsciiMetadataKey::from_bytes(name.as_bytes()) {
                    Ok(key) => {
                        map.append(key, val);
                    }
                    Err(_) => {
                        eprintln!("warning: header {header:?} dropped: invalid metadata key");
                    }
                },
                Err(_) => {
                    eprintln!("warning: header {header:?} dropped: invalid metadata value");
                }
            }
        }
    }

    map
}

/// Try to decode a base64 string using multiple codecs.
///
/// Returns the first successful decode, or None if all fail.
fn try_base64_decode(value: &str) -> Option<Vec<u8>> {
    for (_, engine) in BASE64_ENGINES.iter() {
        if let Ok(decoded) = engine.decode(value.trim()) {
            return Some(decoded);
        }
    }
    None
}

/// Expand `${VAR}` references in header values with environment variable values.
///
/// Equivalent to Go's `ExpandHeaders()` (grpcurl.go).
///
/// Fails if any referenced environment variable is undefined.
pub fn expand_headers(headers: &[String]) -> Result<Vec<String>> {
    let mut result = Vec::with_capacity(headers.len());

    for header in headers {
        let (name, value) = match header.split_once(':') {
            Some((n, v)) => (n, v),
            None => (header.as_str(), ""),
        };

        let expanded = expand_env_vars(value)?;

        if header.contains(':') {
            result.push(format!("{name}:{expanded}"));
        } else {
            result.push(expanded);
        }
    }

    Ok(result)
}

/// Replace all `${VAR}` occurrences with their environment variable values.
fn expand_env_vars(input: &str) -> Result<String> {
    let mut result = String::with_capacity(input.len());
    let mut last_end = 0;

    for cap in ENV_VAR_REGEX.captures_iter(input) {
        let full_match = cap.get(0).expect("regex match exists");
        let var_name = &cap[1];

        // Append text before the match
        result.push_str(&input[last_end..full_match.start()]);

        // Look up the environment variable
        let var_value = std::env::var(var_name).map_err(|_| {
            GrpcurlError::InvalidArgument(format!("no value for environment variable {var_name}"))
        })?;

        result.push_str(&var_value);
        last_end = full_match.end();
    }

    // Append remaining text
    result.push_str(&input[last_end..]);
    Ok(result)
}

/// Format a MetadataMap as a human-readable string.
///
/// Equivalent to Go's `MetadataToString()` (grpcurl.go).
///
/// Output format (one header per line):
/// ```text
/// name: value
/// name: value
/// ```
pub fn metadata_to_string(md: &MetadataMap) -> String {
    if md.is_empty() {
        return "(empty)".to_string();
    }

    let mut lines: Vec<String> = Vec::new();

    for key_and_value in md.iter() {
        match key_and_value {
            tonic::metadata::KeyAndValueRef::Ascii(key, value) => {
                let val_str = value.to_str().unwrap_or("<non-utf8>");
                lines.push(format!("{key}: {val_str}"));
            }
            tonic::metadata::KeyAndValueRef::Binary(key, value) => {
                let bytes = value.to_bytes().unwrap_or_default();
                let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
                lines.push(format!("{key}: {encoded}"));
            }
        }
    }

    lines.sort();
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ascii_header() {
        let headers = vec!["Authorization: Bearer token123".to_string()];
        let md = metadata_from_headers(&headers);
        let val = md.get("authorization").expect("header exists");
        assert_eq!(val.to_str().unwrap(), "Bearer token123");
    }

    #[test]
    fn parse_header_lowercases_name() {
        let headers = vec!["Content-Type: application/grpc".to_string()];
        let md = metadata_from_headers(&headers);
        assert!(md.get("content-type").is_some());
    }

    #[test]
    fn parse_header_no_colon() {
        let headers = vec!["myheader".to_string()];
        let md = metadata_from_headers(&headers);
        let val = md.get("myheader").expect("header exists");
        assert_eq!(val.to_str().unwrap(), "");
    }

    #[test]
    fn parse_header_value_with_colons() {
        let headers = vec!["x-time: 12:34:56".to_string()];
        let md = metadata_from_headers(&headers);
        let val = md.get("x-time").expect("header exists");
        assert_eq!(val.to_str().unwrap(), "12:34:56");
    }

    #[test]
    fn parse_binary_header_base64() {
        // "hello" in standard base64
        let headers = vec!["x-data-bin: aGVsbG8=".to_string()];
        let md = metadata_from_headers(&headers);
        let val = md.get_bin("x-data-bin").expect("binary header exists");
        assert_eq!(val.to_bytes().unwrap().as_ref(), b"hello");
    }

    #[test]
    fn parse_multiple_headers() {
        let headers = vec!["x-first: one".to_string(), "x-second: two".to_string()];
        let md = metadata_from_headers(&headers);
        assert!(md.get("x-first").is_some());
        assert!(md.get("x-second").is_some());
    }

    #[test]
    fn expand_env_vars_in_headers() {
        std::env::set_var("GRPCURL_TEST_TOKEN", "secret123");
        let headers = vec!["Authorization: Bearer ${GRPCURL_TEST_TOKEN}".to_string()];
        let expanded = expand_headers(&headers).unwrap();
        assert_eq!(expanded[0], "Authorization: Bearer secret123");
        std::env::remove_var("GRPCURL_TEST_TOKEN");
    }

    #[test]
    fn expand_env_vars_missing_var_fails() {
        let headers = vec!["x-val: ${GRPCURL_NONEXISTENT_VAR_12345}".to_string()];
        let result = expand_headers(&headers);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("GRPCURL_NONEXISTENT_VAR_12345"));
    }

    #[test]
    fn expand_env_vars_no_expansion_needed() {
        let headers = vec!["x-plain: just a value".to_string()];
        let expanded = expand_headers(&headers).unwrap();
        assert_eq!(expanded[0], "x-plain: just a value");
    }

    #[test]
    fn metadata_to_string_format() {
        let mut md = MetadataMap::new();
        md.insert("x-beta", "two".parse().unwrap());
        md.insert("x-alpha", "one".parse().unwrap());
        let output = metadata_to_string(&md);
        // Should be sorted
        assert!(output.contains("x-alpha: one"));
        assert!(output.contains("x-beta: two"));
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("x-alpha"));
    }

    #[test]
    fn base64_decode_standard() {
        let decoded = try_base64_decode("aGVsbG8=");
        assert_eq!(decoded, Some(b"hello".to_vec()));
    }

    #[test]
    fn base64_decode_no_padding() {
        let decoded = try_base64_decode("aGVsbG8");
        assert_eq!(decoded, Some(b"hello".to_vec()));
    }

    #[test]
    fn base64_decode_invalid() {
        let decoded = try_base64_decode("not!valid!base64!@#$");
        assert!(decoded.is_none());
    }
}
