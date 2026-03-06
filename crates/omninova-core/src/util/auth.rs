use crate::util::crypto::{hmac_sha256_hex, secure_equal_hex};

/// Verify webhook signature header.
///
/// Accepts header formats:
/// - `sha256=<hex>`
/// - `v1=<hex>`
/// - `v0=<hex>`
/// - `t=<unix_ts>,v1=<hex>` (composite style)
/// - `<hex>`
pub fn verify_webhook_signature(
    raw_payload: &str,
    signature_header: Option<&str>,
    signing_secret: &str,
) -> anyhow::Result<bool> {
    verify_webhook_signature_with_policy(
        raw_payload,
        signature_header,
        signing_secret,
        &["sha256", "v1", "v0", "raw"],
    )
}

pub fn verify_webhook_signature_with_policy(
    raw_payload: &str,
    signature_header: Option<&str>,
    signing_secret: &str,
    allowed_algorithms: &[&str],
) -> anyhow::Result<bool> {
    verify_webhook_signature_with_policy_options(
        raw_payload,
        signature_header,
        signing_secret,
        allowed_algorithms,
        &[],
        false,
    )
}

pub fn verify_webhook_signature_with_policy_options(
    raw_payload: &str,
    signature_header: Option<&str>,
    signing_secret: &str,
    allowed_algorithms: &[&str],
    priority_algorithms: &[&str],
    strict_priority: bool,
) -> anyhow::Result<bool> {
    let header = match signature_header {
        Some(v) if !v.trim().is_empty() => v.trim(),
        _ => return Ok(false),
    };

    let allowed = allowed_algorithms
        .iter()
        .map(|s| s.to_ascii_lowercase())
        .collect::<std::collections::HashSet<_>>();
    let signatures = extract_signatures(header);
    if signatures.is_empty() {
        return Ok(false);
    }

    let expected = hmac_sha256_hex(signing_secret, raw_payload.as_bytes())?;
    let signatures_map = signatures
        .iter()
        .map(|(algo, sig)| (algo.to_ascii_lowercase(), sig.to_lowercase()))
        .collect::<std::collections::HashMap<_, _>>();

    if !priority_algorithms.is_empty() {
        for algo in priority_algorithms {
            let algo = algo.to_ascii_lowercase();
            if !allowed.contains(&algo) {
                continue;
            }
            if let Some(sig) = signatures_map.get(&algo) {
                return Ok(secure_equal_hex(sig, &expected));
            }
        }
        if strict_priority {
            return Ok(false);
        }
    }

    for (algo, sig) in signatures {
        if !allowed.contains(&algo) {
            continue;
        }
        if secure_equal_hex(&sig.to_ascii_lowercase(), &expected) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn extract_signatures(header: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if header.contains(',') {
        for part in header.split(',') {
            let p = part.trim();
            if let Some(sig) = p.strip_prefix("sha256=") {
                out.push(("sha256".to_string(), sig.trim().to_string()));
            } else if let Some(sig) = p.strip_prefix("v1=") {
                out.push(("v1".to_string(), sig.trim().to_string()));
            } else if let Some(sig) = p.strip_prefix("v0=") {
                out.push(("v0".to_string(), sig.trim().to_string()));
            }
        }
        return out;
    }
    if let Some(sig) = header.strip_prefix("sha256=") {
        out.push(("sha256".to_string(), sig.trim().to_string()));
        return out;
    }
    if let Some(sig) = header.strip_prefix("v1=") {
        out.push(("v1".to_string(), sig.trim().to_string()));
        return out;
    }
    if let Some(sig) = header.strip_prefix("v0=") {
        out.push(("v0".to_string(), sig.trim().to_string()));
        return out;
    }
    out.push(("raw".to_string(), header.trim().to_string()));
    out
}
