use tonic::metadata::MetadataMap;

/// Extract customer ID from the "authorization" metadata header.
/// Rejects tokens that begin with "agent" (those are for support agents).
pub fn get_customer(metadata: &MetadataMap) -> Option<String> {
    let token = get_auth_code(metadata)?;
    if token.starts_with("agent") {
        return None;
    }
    Some(token)
}

/// Extract agent ID from the "authorization" metadata header.
/// Only accepts tokens that begin with "agent".
pub fn get_agent(metadata: &MetadataMap) -> Option<String> {
    let token = get_auth_code(metadata)?;
    if !token.starts_with("agent") {
        return None;
    }
    Some(token)
}

fn get_auth_code(metadata: &MetadataMap) -> Option<String> {
    let val = metadata.get("authorization")?.to_str().ok()?;
    let lower = val.to_lowercase();
    let (scheme, token) = lower.split_once(' ')?;
    if scheme != "token" {
        return None;
    }
    if token.is_empty() {
        return None;
    }
    Some(token.to_string())
}
