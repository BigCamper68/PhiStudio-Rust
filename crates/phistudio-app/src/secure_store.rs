//! Mobile stores use OS protection and never fall back to plaintext files.
#[cfg(any(target_os = "android", target_os = "ios"))]
use phistudio_plugins::{CredentialStore, validate_secret};
#[cfg(any(target_os = "android", target_os = "ios"))]
use phistudio_sdk::{SecretWrite, valid_id};

pub(crate) fn provider() -> std::sync::Arc<dyn phistudio_plugins::CredentialStore> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        std::sync::Arc::new(MobileStore)
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        std::sync::Arc::new(phistudio_plugins::SystemCredentialStore)
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
struct MobileStore;
#[cfg(any(target_os = "android", target_os = "ios"))]
impl CredentialStore for MobileStore {
    fn supported(&self) -> bool {
        true
    }
    fn read(&self, plugin: &str, key: &str) -> Result<Option<SecretWrite>, String> {
        let Some(value) = call("read", plugin, key, "")? else {
            return Ok(None);
        };
        if value.len() > 32 * 1024 {
            return Err("Protected credential is oversized".into());
        }
        let secret: SecretWrite =
            serde_json::from_str(&value).map_err(|_| "Protected credential is invalid")?;
        if secret.key != key {
            return Err("Protected credential key does not match".into());
        }
        validate_secret(&secret)?;
        Ok(Some(secret))
    }
    fn write(&self, plugin: &str, secret: &SecretWrite) -> Result<(), String> {
        validate_secret(secret)?;
        let payload =
            serde_json::to_string(secret).map_err(|_| "Credential could not be encoded")?;
        call("write", plugin, &secret.key, &payload).map(|_| ())
    }
    fn remove(&self, plugin: &str, key: &str) -> Result<(), String> {
        call("remove", plugin, key, "").map(|_| ())
    }
}

#[cfg(target_os = "android")]
fn call(operation: &str, owner: &str, key: &str, payload: &str) -> Result<Option<String>, String> {
    use jni::objects::{JString, JValue};
    if !valid_id(owner) || !valid_id(key) {
        return Err("Invalid credential owner or key".into());
    }
    crate::android::with_bridge(|env,class| {
        let operation=env.new_string(operation)?;
        let owner=env.new_string(owner)?;
        let key=env.new_string(key)?;
        let payload=env.new_string(payload)?;
        let value=env.call_static_method(class,"credentialStore",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            &[JValue::Object(&operation),JValue::Object(&owner),JValue::Object(&key),JValue::Object(&payload)])?.l()?;
        if value.is_null() { return Ok(None) }
        let value=JString::from(value);
        Ok(Some(String::from(env.get_string(&value)?)))
    }).map_err(|_| "Protected credential operation failed".into())
}

#[cfg(target_os = "ios")]
fn call(operation: &str, owner: &str, key: &str, payload: &str) -> Result<Option<String>, String> {
    use std::ffi::{CStr, CString, c_char};
    unsafe extern "C" {
        fn ps_credentials(
            operation: *const c_char,
            owner: *const c_char,
            key: *const c_char,
            payload: *const c_char,
            output: *mut *mut c_char,
        ) -> i32;
        fn ps_free_credentials(value: *mut c_char);
    }
    if !valid_id(owner) || !valid_id(key) {
        return Err("Invalid credential owner or key".into());
    }
    let operation = CString::new(operation).map_err(|_| "Invalid credential operation")?;
    let owner = CString::new(owner).map_err(|_| "Invalid credential owner")?;
    let key = CString::new(key).map_err(|_| "Invalid credential key")?;
    let payload = CString::new(payload).map_err(|_| "Invalid credential data")?;
    let mut output = std::ptr::null_mut();
    let status = unsafe {
        ps_credentials(
            operation.as_ptr(),
            owner.as_ptr(),
            key.as_ptr(),
            payload.as_ptr(),
            &mut output,
        )
    };
    let result = if output.is_null() {
        Ok(None)
    } else {
        unsafe { CStr::from_ptr(output) }
            .to_str()
            .map(|s| Some(s.to_owned()))
            .map_err(|_| "Protected credential encoding is invalid".into())
    };
    unsafe { ps_free_credentials(output) };
    if status != 0 {
        return Err("Protected credential operation failed".into());
    }
    result
}
