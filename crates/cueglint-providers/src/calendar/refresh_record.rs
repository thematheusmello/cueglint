use super::{CalendarError, CalendarRefreshToken, CalendarResult, Secret};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

// Legacy values are printable ASCII tokens and can never include a newline.
const PREFIX: &str = "cueglint-calendar-refresh-v1\n";
const SCOPE_LEN: usize = 43;

pub(super) fn encode(refresh: &CalendarRefreshToken) -> Zeroizing<String> {
    let mut record = Zeroizing::new(String::with_capacity(
        PREFIX.len() + SCOPE_LEN + 1 + refresh.secret.expose().len(),
    ));
    record.push_str(PREFIX);
    record.push_str(&URL_SAFE_NO_PAD.encode(refresh.grant_scope));
    record.push('\n');
    record.push_str(refresh.secret.expose());
    record
}

pub(super) fn decode(client_key: &str, value: String) -> CalendarResult<CalendarRefreshToken> {
    let value = Zeroizing::new(value);
    let (grant_scope, secret) = if let Some(record) = value.strip_prefix(PREFIX) {
        let (scope, token) = record
            .split_once('\n')
            .filter(|(scope, _)| scope.len() == SCOPE_LEN)
            .ok_or(CalendarError::VaultUnavailable)?;
        let scope = URL_SAFE_NO_PAD
            .decode(scope)
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or(CalendarError::VaultUnavailable)?;
        (scope, token)
    } else {
        // Derive a stable legacy scope without writing during an inspection. The next
        // successful refresh persists this scope alongside any replacement token.
        let mut hash = Sha256::new();
        hash.update(b"cueglint-calendar-legacy-grant-v1\0");
        hash.update(client_key.as_bytes());
        hash.update([0]);
        hash.update(value.as_bytes());
        (hash.finalize().into(), value.as_str())
    };
    Ok(CalendarRefreshToken {
        client_key: client_key.to_owned(),
        secret: Secret::new(secret.to_owned()).map_err(|_| CalendarError::VaultUnavailable)?,
        grant_scope,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_keeps_its_scope_when_the_offline_credential_rotates() {
        let old = decode("fixture-client", "original-legacy-grant".into()).unwrap();
        let again = decode("fixture-client", "original-legacy-grant".into()).unwrap();
        assert_eq!(old.grant_scope, again.grant_scope);
        assert_ne!(
            old.grant_scope,
            decode("fixture-client", "different-legacy-grant".into())
                .unwrap()
                .grant_scope
        );
        let rotated = CalendarRefreshToken {
            secret: Secret::new("original-rotated-grant".into()).unwrap(),
            ..old
        };
        let loaded = decode("fixture-client", encode(&rotated).to_string()).unwrap();
        assert_eq!(loaded.grant_scope, again.grant_scope);
        assert_eq!(loaded.secret.expose(), "original-rotated-grant");
        assert!(decode("fixture-client", format!("{PREFIX}invalid\noriginal")).is_err());
    }
}
