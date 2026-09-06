use crate::constants::dev_extension_id;
use crate::error::BridgeError;

/// Chrome-provided origin is authoritative. Never trust a JSON extension id.
pub fn verify_connecting_origin<I, S>(args: I) -> Result<String, BridgeError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut found = None;
    for arg in args {
        let arg = arg.as_ref();
        if let Some(origin) = parse_chrome_extension_origin(arg) {
            found = Some(origin);
            break;
        }
    }
    let origin = found.ok_or(BridgeError::OriginMissing)?;
    if origin != dev_extension_id() {
        return Err(BridgeError::OriginRejected);
    }
    Ok(origin)
}

fn parse_chrome_extension_origin(arg: &str) -> Option<String> {
    let rest = arg.strip_prefix("chrome-extension://")?;
    let id = rest.split('/').next().unwrap_or(rest);
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_origin_is_accepted() {
        let origin = format!("chrome-extension://{}/", crate::constants::dev_extension_id());
        assert!(verify_connecting_origin(["host.exe", &origin]).is_ok());
    }

    #[test]
    fn wrong_origin_is_rejected() {
        let err = verify_connecting_origin(["host.exe", "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/"])
            .unwrap_err();
        assert!(matches!(err, BridgeError::OriginRejected));
        assert!(!err.to_string().contains("secret"));
    }

    #[test]
    fn missing_origin_is_rejected() {
        let err = verify_connecting_origin(["host.exe", "--parent-window=1"]).unwrap_err();
        assert!(matches!(err, BridgeError::OriginMissing));
    }

    #[test]
    fn json_supplied_extension_id_is_ignored() {
        let err = verify_connecting_origin([
            "host.exe",
            r#"{"extension_id":"caooogobppgihkdpcjibhoinkfobenhe"}"#,
        ])
        .unwrap_err();
        assert!(matches!(err, BridgeError::OriginMissing));
    }
}
