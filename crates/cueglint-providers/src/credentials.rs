use crate::{ErrorKind, ProviderError, ProviderResult};
use cueglint_core::{CredentialRef, CredentialStatus, ProviderConfig, ProviderId};
use std::fmt;
use zeroize::Zeroizing;

pub struct Secret(Zeroizing<String>);

impl Secret {
    pub fn new(value: String) -> ProviderResult<Self> {
        let value = Zeroizing::new(value);
        if value.is_empty() || value.len() > 8192 || !value.bytes().all(|b| (33..=126).contains(&b))
        {
            return Err(ProviderError::new(ErrorKind::InvalidConfig));
        }
        Ok(Self(value))
    }
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret([REDACTED])")
    }
}

pub trait CredentialStore: Send {
    fn supported(&self) -> bool {
        true
    }
    fn status(&self, reference: &CredentialRef) -> CredentialStatus;
    fn store(&self, reference: &CredentialRef, secret: Secret) -> ProviderResult<()>;
    fn delete(&self, reference: &CredentialRef) -> ProviderResult<()>;
    fn load(&self, reference: &CredentialRef) -> ProviderResult<Option<Secret>>;
}

#[derive(Default)]
pub struct NativeCredentialStore;

impl NativeCredentialStore {
    pub fn supported(&self) -> bool {
        cfg!(any(target_os = "macos", target_os = "windows"))
    }
    pub fn status(&self, reference: &CredentialRef) -> CredentialStatus {
        CredentialStore::status(self, reference)
    }
    pub fn store(&self, reference: &CredentialRef, secret: Secret) -> ProviderResult<()> {
        CredentialStore::store(self, reference, secret)
    }
    pub fn delete(&self, reference: &CredentialRef) -> ProviderResult<()> {
        CredentialStore::delete(self, reference)
    }
    pub fn load(&self, reference: &CredentialRef) -> ProviderResult<Option<Secret>> {
        CredentialStore::load(self, reference)
    }
}

/// Called only after an explicit connect/list/generate action; never downgrades a key-read failure.
pub fn load_for_config(
    store: &dyn CredentialStore,
    config: &ProviderConfig,
) -> ProviderResult<Option<Secret>> {
    config
        .validate()
        .map_err(|_| ProviderError::new(ErrorKind::InvalidConfig))?;
    if config.provider.is_local() {
        return Err(ProviderError::new(ErrorKind::InvalidConfig));
    }
    if config.provider == ProviderId::OpenAiCompatible && !store.supported() {
        return Ok(None);
    }
    let reference = config
        .credential_ref()
        .map_err(|_| ProviderError::new(ErrorKind::InvalidConfig))?;
    let key = store.load(&reference)?;
    if key.is_none() && config.provider != ProviderId::OpenAiCompatible {
        return Err(ProviderError::new(ErrorKind::MissingCredential));
    }
    Ok(key)
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn entry(reference: &CredentialRef) -> ProviderResult<keyring::Entry> {
    use sha2::{Digest, Sha256};
    let account = format!(
        "{}-{:x}",
        reference.provider().as_str(),
        Sha256::digest(reference.endpoint().as_bytes())
    );
    keyring::Entry::new("org.cueglint.provider-key.v1", &account)
        .map_err(|_| ProviderError::new(ErrorKind::CredentialUnavailable))
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl CredentialStore for NativeCredentialStore {
    fn status(&self, reference: &CredentialRef) -> CredentialStatus {
        match self.load(reference) {
            Ok(Some(_)) => CredentialStatus::Stored,
            Ok(None) => CredentialStatus::Missing,
            Err(_) => CredentialStatus::Unavailable {
                reason: ProviderError::new(ErrorKind::CredentialUnavailable).to_string(),
            },
        }
    }
    fn store(&self, reference: &CredentialRef, secret: Secret) -> ProviderResult<()> {
        entry(reference)?
            .set_password(secret.expose())
            .map_err(|_| ProviderError::new(ErrorKind::CredentialUnavailable))
    }
    fn delete(&self, reference: &CredentialRef) -> ProviderResult<()> {
        match entry(reference)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(ProviderError::new(ErrorKind::CredentialUnavailable)),
        }
    }
    fn load(&self, reference: &CredentialRef) -> ProviderResult<Option<Secret>> {
        match entry(reference)?.get_password() {
            Ok(secret) => Secret::new(secret).map(Some),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(ProviderError::new(ErrorKind::CredentialUnavailable)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    #[derive(Default)]
    struct Memory {
        unavailable: bool,
        unsupported: bool,
        reads: AtomicUsize,
        keys: Mutex<Vec<(CredentialRef, Secret)>>,
    }
    impl CredentialStore for Memory {
        fn supported(&self) -> bool {
            !self.unsupported
        }
        fn status(&self, reference: &CredentialRef) -> CredentialStatus {
            if self.unavailable || self.unsupported {
                CredentialStatus::Unavailable {
                    reason: "Original unavailable-store fixture".to_owned(),
                }
            } else if self
                .keys
                .lock()
                .unwrap()
                .iter()
                .any(|(key, _)| key == reference)
            {
                CredentialStatus::Stored
            } else {
                CredentialStatus::Missing
            }
        }
        fn load(&self, reference: &CredentialRef) -> ProviderResult<Option<Secret>> {
            self.reads.fetch_add(1, Ordering::Relaxed);
            if self.unavailable || self.unsupported {
                return Err(ProviderError::new(ErrorKind::CredentialUnavailable));
            }
            self.keys
                .lock()
                .unwrap()
                .iter()
                .find(|(key, _)| key == reference)
                .map(|(_, secret)| Secret::new(secret.expose().to_owned()))
                .transpose()
        }
        fn store(&self, reference: &CredentialRef, secret: Secret) -> ProviderResult<()> {
            self.delete(reference)?;
            self.keys.lock().unwrap().push((reference.clone(), secret));
            Ok(())
        }
        fn delete(&self, reference: &CredentialRef) -> ProviderResult<()> {
            self.keys
                .lock()
                .unwrap()
                .retain(|(key, _)| key != reference);
            Ok(())
        }
    }

    fn config(provider: ProviderId) -> ProviderConfig {
        ProviderConfig {
            provider,
            model_id: "original-test-model".to_owned(),
            base_url: (provider == ProviderId::OpenAiCompatible)
                .then(|| "http://127.0.0.1:1234/v1/".to_owned()),
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "writes a synthetic credential in an isolated Windows account"]
    fn windows_native_credential_round_trip() -> anyhow::Result<()> {
        use anyhow::{Context, ensure};
        use std::time::{SystemTime, UNIX_EPOCH};

        ensure!(
            std::env::var("CUEGLINT_TEST_WINDOWS_VAULT").ok().as_deref() == Some("1"),
            "Set CUEGLINT_TEST_WINDOWS_VAULT=1 in a disposable Windows account to opt in."
        );
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let mut config = config(ProviderId::OpenAiCompatible);
        config.base_url = Some(format!(
            "http://127.0.0.1:1234/cueglint-windows-vault-test-{}-{timestamp}/v1/",
            std::process::id()
        ));
        let reference = config.credential_ref()?;
        let vault = NativeCredentialStore;
        let initial = vault.status(&reference);
        ensure!(
            initial == CredentialStatus::Missing,
            "Expected a fresh fixture scope. Windows returned {initial:?}."
        );
        eprintln!("Windows credential fixture uses {}", reference.endpoint());
        const SYNTHETIC: &str = "cueglint-original-windows-vault-fixture";
        let round_trip = (|| -> anyhow::Result<()> {
            vault
                .store(&reference, Secret::new(SYNTHETIC.to_owned())?)
                .context("Store the synthetic Windows credential")?;
            ensure!(
                vault.status(&reference) == CredentialStatus::Stored,
                "Expected Stored after writing the synthetic credential."
            );
            let loaded = load_for_config(&vault, &config).context("Read the fixture credential")?;
            ensure!(
                loaded.is_some_and(|secret| secret.expose() == SYNTHETIC),
                "Windows returned a different fixture credential."
            );
            Ok(())
        })();
        let removal = vault.delete(&reference);
        let final_status = vault.status(&reference);
        removal.with_context(|| {
            format!("Remove the fixture credential for {}", reference.endpoint())
        })?;
        ensure!(
            final_status == CredentialStatus::Missing,
            "Expected Missing after cleaning up {}. Windows returned {final_status:?}.",
            reference.endpoint()
        );
        round_trip
    }

    #[test]
    fn key_storage_is_separate_from_activation_exports_and_provider_or_endpoint_scope()
    -> anyhow::Result<()> {
        let memory = Memory::default();
        let mut store = cueglint_core::Store::open_in_memory()?;
        let session = store.start_session("Original credential boundary fixture", None)?;
        store.update_notes(&session.id, "Preserve original user notes.")?;
        let secret = "cueglint-original-private-credential-marker";
        let config = config(ProviderId::OpenAiCompatible);
        let reference = config.credential_ref()?;
        memory.store(&reference, Secret::new(secret.to_owned())?)?;
        assert_eq!(memory.status(&reference), CredentialStatus::Stored);
        assert!(store.settings()?.answer_provider.provider.is_local());
        assert_eq!(
            store.session(&session.id)?.notes,
            "Preserve original user notes."
        );
        assert!(!store.backup_json()?.contains(secret));
        assert!(
            !store
                .export_session(&session.id, cueglint_core::ExportFormat::Json)?
                .contains(secret)
        );
        assert!(!serde_json::to_string(&memory.status(&reference))?.contains(secret));
        let key = load_for_config(&memory, &config)?.unwrap();
        assert_eq!(key.expose(), secret);
        assert!(!format!("{key:?}").contains(secret));
        let mut changed = config.clone();
        changed.base_url = Some("http://127.0.0.1:5678/v1/".to_owned());
        assert!(load_for_config(&memory, &changed)?.is_none());
        let mut changed = config.clone();
        changed.base_url = Some("http://127.0.0.1:1234/different-base/".to_owned());
        assert!(load_for_config(&memory, &changed)?.is_none());
        memory.delete(&reference)?;
        assert_eq!(memory.status(&reference), CredentialStatus::Missing);
        assert!(load_for_config(&memory, &config)?.is_none());
        Ok(())
    }

    #[test]
    fn optional_compatible_keys_never_hide_supported_store_failures() {
        let missing = Memory::default();
        assert_eq!(
            load_for_config(&missing, &config(ProviderId::OpenAi))
                .unwrap_err()
                .kind,
            ErrorKind::MissingCredential
        );
        assert!(
            load_for_config(&missing, &config(ProviderId::OpenAiCompatible))
                .unwrap()
                .is_none()
        );
        let unavailable = Memory {
            unavailable: true,
            ..Memory::default()
        };
        assert_eq!(
            load_for_config(&unavailable, &config(ProviderId::OpenAiCompatible))
                .unwrap_err()
                .kind,
            ErrorKind::CredentialUnavailable
        );
        let unsupported = Memory {
            unsupported: true,
            ..Memory::default()
        };
        assert!(
            load_for_config(&unsupported, &config(ProviderId::OpenAiCompatible))
                .unwrap()
                .is_none()
        );
        assert_eq!(unsupported.reads.load(Ordering::Relaxed), 0);
        assert!(matches!(
            unsupported.status(
                &config(ProviderId::OpenAiCompatible)
                    .credential_ref()
                    .unwrap()
            ),
            CredentialStatus::Unavailable { .. }
        ));
        assert_eq!(
            load_for_config(&unsupported, &config(ProviderId::Anthropic))
                .unwrap_err()
                .kind,
            ErrorKind::CredentialUnavailable
        );
    }

    #[test]
    fn secrets_reject_header_injection_and_never_format_their_content() {
        for value in [
            "",
            "key\nAuthorization: stolen",
            "key\0tail",
            " leading-space",
            "trailing-space ",
        ] {
            assert_eq!(
                Secret::new(value.to_owned()).unwrap_err().kind,
                ErrorKind::InvalidConfig
            );
        }
        assert_eq!(
            format!(
                "{:?}",
                Secret::new("original-fixture-key".to_owned()).unwrap()
            ),
            "Secret([REDACTED])"
        );
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl CredentialStore for NativeCredentialStore {
    fn supported(&self) -> bool {
        false
    }
    fn status(&self, _: &CredentialRef) -> CredentialStatus {
        CredentialStatus::Unavailable { reason: "Native provider key storage is currently supported on macOS and Windows. No plaintext fallback is used.".to_owned() }
    }
    fn store(&self, _: &CredentialRef, _: Secret) -> ProviderResult<()> {
        Err(ProviderError::new(ErrorKind::CredentialUnavailable))
    }
    fn delete(&self, _: &CredentialRef) -> ProviderResult<()> {
        Err(ProviderError::new(ErrorKind::CredentialUnavailable))
    }
    fn load(&self, _: &CredentialRef) -> ProviderResult<Option<Secret>> {
        Err(ProviderError::new(ErrorKind::CredentialUnavailable))
    }
}
