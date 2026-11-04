use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JClass, JString, JValue},
};
use std::{
    path::PathBuf,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
};

pub static WAS_SUSPENDED: AtomicBool = AtomicBool::new(false);
pub static BACK_REQUESTED: AtomicBool = AtomicBool::new(false);
pub static DENSITY: AtomicU32 = AtomicU32::new(1_f32.to_bits());
struct Bridge {
    vm: JavaVM,
    class: GlobalRef,
}
static BRIDGE: OnceLock<Bridge> = OnceLock::new();
static DOCUMENT_RESULTS: Mutex<Vec<DocumentResult>> = Mutex::new(Vec::new());
pub(crate) fn with_bridge<T>(
    f: impl FnOnce(&mut JNIEnv<'_>, &JClass<'_>) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let bridge = BRIDGE
        .get()
        .ok_or_else(|| anyhow::anyhow!("Android bridge is not initialized"))?;
    let mut env = bridge.vm.attach_current_thread()?;
    let class: &JClass<'_> = bridge.class.as_obj().into();
    let result = f(&mut env, class);
    if env.exception_check()? {
        let exception = env.exception_occurred()?;
        env.exception_clear()?;
        let message = env
            .call_method(exception, "toString", "()Ljava/lang/String;", &[])?
            .l()?;
        let message = String::from(env.get_string((&message).into())?);
        anyhow::bail!("{message}");
    }
    result
}
pub struct DocumentResult {
    pub token: u64,
    pub path: Option<PathBuf>,
    pub name: String,
    pub error: Option<String>,
    pub canceled: bool,
}
pub fn document_results() -> Vec<DocumentResult> {
    std::mem::take(&mut *DOCUMENT_RESULTS.lock().unwrap_or_else(|e| e.into_inner()))
}

pub fn request_package(token: u64) -> anyhow::Result<()> {
    let bridge = BRIDGE
        .get()
        .ok_or_else(|| anyhow::anyhow!("Android file picker is not initialized"))?;
    let mut env = bridge.vm.attach_current_thread()?;
    let class: &JClass<'_> = bridge.class.as_obj().into();
    let result = env.call_static_method(
        class,
        "requestPackage",
        "(J)V",
        &[JValue::Long(token as i64)],
    );
    if env.exception_check()? {
        env.exception_clear()?;
        anyhow::bail!("Could not open the system file picker");
    }
    result?;
    Ok(())
}

// Miniquad exports JNI_OnLoad and owns the JavaVM used by its callbacks.
// A second definition here would conflict when linking the Android cdylib.

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_phistudio_rust_MainActivity_setStorageRoot(
    mut env: JNIEnv,
    class: JClass,
    path: JString,
) {
    if let (Ok(vm), Ok(class)) = (env.get_java_vm(), env.new_global_ref(class)) {
        let _ = BRIDGE.set(Bridge { vm, class });
    }
    let path = match env.get_string(&path) {
        Ok(path) => String::from(path),
        Err(_) => return,
    };
    if let Err(error) = std::env::set_current_dir(&path) {
        let _ = env.throw_new("java/io/IOException", error.to_string());
        return;
    }
    // Android's default /data/local/tmp is not writable by ordinary apps.
    // Configure tempfile before the editor and its plugin workers are started.
    let temporary = std::path::Path::new(&path).join(".tmp");
    if let Err(error) = std::fs::create_dir_all(&temporary) {
        let _ = env.throw_new("java/io/IOException", error.to_string());
        return;
    }
    if let Err(existing) = tempfile::env::override_temp_dir(&temporary)
        && existing != temporary
    {
        let _ = env.throw_new("java/io/IOException", "Temporary storage root changed");
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_phistudio_rust_MainActivity_documentResult(
    mut env: JNIEnv,
    _: JClass,
    token: i64,
    status: i32,
    path: JString,
    name: JString,
    error: JString,
) {
    let read = |env: &mut JNIEnv, value: &JString| -> String {
        env.get_string(value).map(String::from).unwrap_or_default()
    };
    let path = read(&mut env, &path);
    let name = read(&mut env, &name);
    let error = read(&mut env, &error);
    DOCUMENT_RESULTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(DocumentResult {
            token: token as u64,
            path: (status == 1 && !path.is_empty()).then(|| PathBuf::from(path)),
            name,
            error: (status < 0).then(|| {
                if error.is_empty() {
                    "Could not access the document".into()
                } else {
                    error
                }
            }),
            canceled: status == 0,
        });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_phistudio_rust_MainActivity_notifySuspended(_: JNIEnv, _: JClass) {
    crate::video_encoder::cancel();
    phistudio_prpr::suspend_audio();
    WAS_SUSPENDED.store(true, Ordering::Release);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_phistudio_rust_MainActivity_notifyBackRequested(
    _: JNIEnv,
    _: JClass,
) {
    BACK_REQUESTED.store(true, Ordering::Release);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_phistudio_rust_MainActivity_setDisplayDensity(
    _: JNIEnv,
    _: JClass,
    density: f32,
) {
    if density.is_finite() && density > 0. {
        DENSITY.store(density.to_bits(), Ordering::Release);
    }
}

pub fn request_export_package(
    path: &std::path::Path,
    name: &str,
    token: u64,
) -> anyhow::Result<()> {
    let bridge = BRIDGE
        .get()
        .ok_or_else(|| anyhow::anyhow!("Android file picker is not initialized"))?;
    let mut env = bridge.vm.attach_current_thread()?;
    let class: &JClass<'_> = bridge.class.as_obj().into();
    let path = env.new_string(path.to_string_lossy())?;
    let name = env.new_string(name)?;
    let result = env.call_static_method(
        class,
        "requestExportPackage",
        "(Ljava/lang/String;Ljava/lang/String;J)V",
        &[
            JValue::Object(&path),
            JValue::Object(&name),
            JValue::Long(token as i64),
        ],
    );
    if env.exception_check()? {
        env.exception_clear()?;
        anyhow::bail!("Could not open the package export picker");
    }
    result?;
    Ok(())
}

pub fn request_export_video(path: &std::path::Path, name: &str, token: u64) -> anyhow::Result<()> {
    with_bridge(|env, class| {
        let path = env.new_string(path.to_string_lossy())?;
        let name = env.new_string(name)?;
        env.call_static_method(
            class,
            "requestExportVideo",
            "(Ljava/lang/String;Ljava/lang/String;J)V",
            &[
                JValue::Object(&path),
                JValue::Object(&name),
                JValue::Long(token as i64),
            ],
        )?;
        Ok(())
    })
}
pub fn request_export_file(path: &std::path::Path, name: &str, token: u64) -> anyhow::Result<()> {
    with_bridge(|env, class| {
        let path = env.new_string(path.to_string_lossy())?;
        let name = env.new_string(name)?;
        env.call_static_method(
            class,
            "requestExportFile",
            "(Ljava/lang/String;Ljava/lang/String;J)V",
            &[
                JValue::Object(&path),
                JValue::Object(&name),
                JValue::Long(token as i64),
            ],
        )?;
        Ok(())
    })
}

pub fn request_media(kind: &str, token: u64) -> anyhow::Result<()> {
    let bridge = BRIDGE
        .get()
        .ok_or_else(|| anyhow::anyhow!("Android file picker is not initialized"))?;
    let mut env = bridge.vm.attach_current_thread()?;
    let class: &JClass<'_> = bridge.class.as_obj().into();
    let kind = env.new_string(kind)?;
    let result = env.call_static_method(
        class,
        "requestMedia",
        "(Ljava/lang/String;J)V",
        &[JValue::Object(&kind), JValue::Long(token as i64)],
    );
    if env.exception_check()? {
        env.exception_clear()?;
        anyhow::bail!("Could not open the media file picker");
    }
    result?;
    Ok(())
}
