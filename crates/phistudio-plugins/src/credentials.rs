//! Protected storage is supplied by the platform, with no file/plaintext fallback.
use phistudio_sdk::{NetworkPermissions, SecretWrite, valid_id};
use std::sync::{Arc, Mutex};

const INDEX_KEY: &str = "phistudio-host-credential-index";
const MAX_KEYS: usize = 64;

/// A protected index bounds persistent records, including across restarts.
/// Reserve the slot before writing data: an interrupted write can lose a slot,
/// but cannot leave an uncounted secret that bypasses the quota.
pub(crate) struct BoundedCredentialStore {
    inner: Arc<dyn CredentialStore>,
    locks: [Mutex<()>; 16],
}
impl BoundedCredentialStore {
    pub(crate) fn new(inner: Arc<dyn CredentialStore>) -> Self {
        Self {
            inner,
            locks: std::array::from_fn(|_| Mutex::new(())),
        }
    }
    fn owner_lock(&self, owner: &str) -> Result<std::sync::MutexGuard<'_, ()>, String> {
        let hash = owner
            .bytes()
            .fold(0usize, |n, b| n.wrapping_mul(31).wrapping_add(b as usize));
        self.locks[hash % self.locks.len()]
            .lock()
            .map_err(|_| "Protected credential store interrupted".into())
    }
    fn index(&self, owner: &str) -> Result<Vec<String>, String> {
        let keys = match self.inner.read(owner, INDEX_KEY)? {
            Some(record) => serde_json::from_str::<Vec<String>>(&record.value)
                .map_err(|_| "Protected credential index is invalid")?,
            None => vec![],
        };
        if keys.len() > MAX_KEYS
            || keys.iter().any(|k| !valid_id(k) || k == INDEX_KEY)
            || keys.iter().collect::<std::collections::BTreeSet<_>>().len() != keys.len()
        {
            return Err("Protected credential index is invalid".into());
        }
        Ok(keys)
    }
    fn save_index(&self, owner: &str, keys: &[String]) -> Result<(), String> {
        self.inner.write(
            owner,
            &SecretWrite {
                key: INDEX_KEY.into(),
                value: serde_json::to_string(keys)
                    .map_err(|_| "Credential index could not be encoded")?,
                origins: Default::default(),
            },
        )
    }
}
impl CredentialStore for BoundedCredentialStore {
    fn supported(&self) -> bool {
        self.inner.supported()
    }
    fn read(&self, plugin: &str, key: &str) -> Result<Option<SecretWrite>, String> {
        validate(plugin, key)?;
        if key == INDEX_KEY {
            return Err("Reserved credential key".into());
        }
        let _guard = self.owner_lock(plugin)?;
        self.inner.read(plugin, key)
    }
    fn write(&self, plugin: &str, secret: &SecretWrite) -> Result<(), String> {
        validate(plugin, &secret.key)?;
        validate_secret(secret)?;
        if secret.key == INDEX_KEY {
            return Err("Reserved credential key".into());
        }
        let _guard = self.owner_lock(plugin)?;
        let mut keys = self.index(plugin)?;
        if !keys.contains(&secret.key) {
            if keys.len() >= MAX_KEYS {
                return Err("A plugin may store at most 64 credentials".into());
            }
            keys.push(secret.key.clone());
            self.save_index(plugin, &keys)?;
        }
        self.inner.write(plugin, secret)
    }
    fn remove(&self, plugin: &str, key: &str) -> Result<(), String> {
        validate(plugin, key)?;
        if key == INDEX_KEY {
            return Err("Reserved credential key".into());
        }
        let _guard = self.owner_lock(plugin)?;
        self.inner.remove(plugin, key)?;
        let mut keys = self.index(plugin)?;
        keys.retain(|k| k != key);
        self.save_index(plugin, &keys)
    }
}

pub trait CredentialStore: Send + Sync {
    fn supported(&self) -> bool;
    fn read(&self, plugin: &str, key: &str) -> Result<Option<SecretWrite>, String>;
    fn write(&self, plugin: &str, secret: &SecretWrite) -> Result<(), String>;
    fn remove(&self, plugin: &str, key: &str) -> Result<(), String>;
}

pub struct SystemCredentialStore;

fn validate(plugin: &str, key: &str) -> Result<(), String> {
    if !valid_id(plugin) || !valid_id(key) {
        return Err("Invalid credential owner or key".into());
    }
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
impl CredentialStore for SystemCredentialStore {
    fn supported(&self) -> bool {
        true
    }
    fn read(&self, plugin: &str, key: &str) -> Result<Option<SecretWrite>, String> {
        validate(plugin, key)?;
        let entry = keyring::Entry::new(&format!("PhiStudio.plugins.{plugin}"), key)
            .map_err(|_| "Protected credential store is unavailable")?;
        let bytes = match entry.get_secret() {
            Ok(bytes) => bytes,
            Err(keyring::Error::NoEntry) => return Ok(None),
            Err(_) => return Err("Protected credential could not be read".into()),
        };
        if bytes.len() > 32 * 1024 {
            return Err("Protected credential is oversized".into());
        }
        let secret: SecretWrite =
            serde_json::from_slice(&bytes).map_err(|_| "Protected credential is invalid")?;
        if secret.key != key {
            return Err("Protected credential key does not match".into());
        }
        validate_secret(&secret)?;
        Ok(Some(secret))
    }
    fn write(&self, plugin: &str, secret: &SecretWrite) -> Result<(), String> {
        validate(plugin, &secret.key)?;
        validate_secret(secret)?;
        let bytes = serde_json::to_vec(secret).map_err(|_| "Credential could not be encoded")?;
        keyring::Entry::new(&format!("PhiStudio.plugins.{plugin}"), &secret.key)
            .map_err(|_| "Protected credential store is unavailable")?
            .set_secret(&bytes)
            .map_err(|_| "Protected credential could not be saved".into())
    }
    fn remove(&self, plugin: &str, key: &str) -> Result<(), String> {
        validate(plugin, key)?;
        match keyring::Entry::new(&format!("PhiStudio.plugins.{plugin}"), key)
            .map_err(|_| "Protected credential store is unavailable")?
            .delete_credential()
        {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err("Protected credential could not be removed".into()),
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
impl CredentialStore for SystemCredentialStore {
    fn supported(&self) -> bool {
        false
    }
    fn read(&self, plugin: &str, key: &str) -> Result<Option<SecretWrite>, String> {
        validate(plugin, key)?;
        Err("Protected credential store has not been configured on this platform".into())
    }
    fn write(&self, plugin: &str, secret: &SecretWrite) -> Result<(), String> {
        validate(plugin, &secret.key)?;
        Err("Protected credential store has not been configured on this platform".into())
    }
    fn remove(&self, plugin: &str, key: &str) -> Result<(), String> {
        validate(plugin, key)?;
        Err("Protected credential store has not been configured on this platform".into())
    }
}

pub fn validate_secret(secret: &SecretWrite) -> Result<(), String> {
    if !valid_id(&secret.key) || secret.value.is_empty() || secret.value.len() > 16 * 1024 {
        return Err("Credentials require a valid key and 1–16384 bytes".into());
    }
    if crate::limits::serialized_size(secret, 32 * 1024).is_none() {
        return Err("Protected credential record exceeds 32 KiB".into());
    }
    NetworkPermissions {
        origins: secret.origins.clone(),
        local_network: true,
    }
    .validate()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Memory(Mutex<std::collections::BTreeMap<(String, String), SecretWrite>>);
    impl CredentialStore for Memory {
        fn supported(&self) -> bool {
            true
        }
        fn read(&self, p: &str, k: &str) -> Result<Option<SecretWrite>, String> {
            Ok(self.0.lock().unwrap().get(&(p.into(), k.into())).cloned())
        }
        fn write(&self, p: &str, s: &SecretWrite) -> Result<(), String> {
            self.0
                .lock()
                .unwrap()
                .insert((p.into(), s.key.clone()), s.clone());
            Ok(())
        }
        fn remove(&self, p: &str, k: &str) -> Result<(), String> {
            self.0.lock().unwrap().remove(&(p.into(), k.into()));
            Ok(())
        }
    }
    #[test]
    fn credential_quota_survives_restart_is_per_owner_and_reclaims_deleted_slots() {
        let memory = Arc::new(Memory::default());
        let store = BoundedCredentialStore::new(memory.clone());
        for index in 0..64 {
            store
                .write(
                    "test.owner",
                    &SecretWrite {
                        key: format!("token-{index}"),
                        value: "secret".into(),
                        origins: Default::default(),
                    },
                )
                .unwrap();
        }
        let store = BoundedCredentialStore::new(memory);
        let token = SecretWrite {
            key: "extra".into(),
            value: "secret".into(),
            origins: Default::default(),
        };
        assert!(store.write("test.owner", &token).is_err());
        store.write("test.other", &token).unwrap();
        store.remove("test.owner", "token-0").unwrap();
        store.write("test.owner", &token).unwrap();
        assert!(store.read("test.owner", INDEX_KEY).is_err());
        assert!(store.remove("test.owner", INDEX_KEY).is_err());
        assert!(store.read("test.other", "token-1").unwrap().is_none());
    }
}
