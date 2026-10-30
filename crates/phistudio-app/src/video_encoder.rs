//! Blocking encoders run only on the bounded movie worker, never on the GL thread.
use anyhow::Result;
use phistudio_core::video::{VideoPlan, VideoSettings};
use std::path::Path;

pub struct Encoder(backend::Encoder);
impl Encoder {
    pub fn new(
        path: &Path,
        wav: Option<&Path>,
        settings: &VideoSettings,
        plan: VideoPlan,
    ) -> Result<Self> {
        backend::Encoder::new(path, wav, settings, plan).map(Self)
    }
    pub fn frame(&mut self, rgba: &[u8], index: u64) -> Result<()> {
        self.0.frame(rgba, index)
    }
    pub fn finish(&mut self) -> Result<()> {
        self.0.finish()
    }
}
pub fn reset() {
    backend::reset();
}
pub fn cancel() {
    backend::cancel();
}
pub fn release() {
    backend::release();
}
#[cfg(any(target_os = "android", target_os = "ios"))]
fn configuration(path: &Path, wav: Option<&Path>, s: &VideoSettings, p: VideoPlan) -> String {
    serde_json::json!({"path":path,"wav":wav,"width":p.width,"height":p.height,"fps":p.fps,"frames":p.frames,"bitrate":s.bitrate_mbps*1_000_000,"audioBitrate":s.audio_bitrate_kbps*1000}).to_string()
}
#[cfg(target_os = "android")]
mod backend {
    use super::*;
    use jni::objects::{GlobalRef, JValue};
    pub struct Encoder(GlobalRef);
    pub fn reset() {
        let _ = crate::android::with_bridge(|env, c| {
            env.call_static_method(c, "videoReset", "()V", &[])?;
            Ok(())
        });
    }
    pub fn cancel() {
        let _ = crate::android::with_bridge(|env, c| {
            env.call_static_method(c, "videoCancel", "()V", &[])?;
            Ok(())
        });
    }
    pub fn release() {
        let _ = crate::android::with_bridge(|env, c| {
            env.call_static_method(c, "videoRelease", "()V", &[])?;
            Ok(())
        });
    }
    impl Encoder {
        pub fn new(
            path: &Path,
            wav: Option<&Path>,
            s: &VideoSettings,
            p: VideoPlan,
        ) -> Result<Self> {
            crate::android::with_bridge(|env, c| {
                let json = env.new_string(configuration(path, wav, s, p))?;
                let object = env
                    .call_static_method(
                        c,
                        "videoCreate",
                        "(Ljava/lang/String;)Lcom/phistudio/rust/VideoEncoder;",
                        &[JValue::Object(&json)],
                    )?
                    .l()?;
                Ok(Self(env.new_global_ref(object)?))
            })
        }
        pub fn frame(&mut self, rgba: &[u8], index: u64) -> Result<()> {
            crate::android::with_bridge(|env, _| {
                let bytes = env.byte_array_from_slice(rgba)?;
                env.call_method(
                    self.0.as_obj(),
                    "frame",
                    "([BJ)V",
                    &[JValue::Object(&bytes), JValue::Long(index as i64)],
                )?;
                Ok(())
            })
        }
        pub fn finish(&mut self) -> Result<()> {
            crate::android::with_bridge(|env, _| {
                env.call_method(self.0.as_obj(), "finish", "()V", &[])?;
                Ok(())
            })
        }
    }
    impl Drop for Encoder {
        fn drop(&mut self) {
            let _ = crate::android::with_bridge(|env, _| {
                env.call_method(self.0.as_obj(), "close", "()V", &[])?;
                Ok(())
            });
        }
    }
}
#[cfg(target_os = "ios")]
mod backend {
    use super::*;
    use std::ffi::{CStr, CString, c_char, c_void};
    unsafe extern "C" {
        fn ps_video_reset();
        fn ps_video_cancel();
        fn ps_video_release();
        fn ps_video_create(json: *const c_char) -> *mut c_void;
        fn ps_video_frame(handle: *mut c_void, data: *const u8, length: usize, index: u64) -> i32;
        fn ps_video_finish(handle: *mut c_void) -> i32;
        fn ps_video_error() -> *const c_char;
        fn ps_video_close(handle: *mut c_void);
    }
    pub struct Encoder(*mut c_void);
    pub fn reset() {
        unsafe { ps_video_reset() }
    }
    pub fn cancel() {
        unsafe { ps_video_cancel() }
    }
    pub fn release() {
        unsafe { ps_video_release() }
    }
    fn error() -> anyhow::Error {
        unsafe {
            let s = ps_video_error();
            anyhow::anyhow!(
                "{}",
                if s.is_null() {
                    "Video encoder failed".into()
                } else {
                    CStr::from_ptr(s).to_string_lossy()
                }
            )
        }
    }
    impl Encoder {
        pub fn new(
            path: &Path,
            wav: Option<&Path>,
            s: &VideoSettings,
            p: VideoPlan,
        ) -> Result<Self> {
            let json = CString::new(configuration(path, wav, s, p))?;
            let handle = unsafe { ps_video_create(json.as_ptr()) };
            if handle.is_null() {
                Err(error())
            } else {
                Ok(Self(handle))
            }
        }
        pub fn frame(&mut self, rgba: &[u8], index: u64) -> Result<()> {
            if unsafe { ps_video_frame(self.0, rgba.as_ptr(), rgba.len(), index) } == 0 {
                Err(error())
            } else {
                Ok(())
            }
        }
        pub fn finish(&mut self) -> Result<()> {
            if unsafe { ps_video_finish(self.0) } == 0 {
                Err(error())
            } else {
                Ok(())
            }
        }
    }
    impl Drop for Encoder {
        fn drop(&mut self) {
            unsafe { ps_video_close(self.0) }
        }
    }
}
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod backend {
    use super::*;
    use anyhow::{Context, ensure};
    use std::sync::{Arc, Mutex, Weak};
    use std::{
        io::Write,
        process::{Child, ChildStdin, Command, Stdio},
    };
    static ACTIVE: Mutex<Weak<Mutex<Child>>> = Mutex::new(Weak::new());
    pub struct Encoder {
        child: Arc<Mutex<Child>>,
        input: Option<ChildStdin>,
        log: std::fs::File,
    }
    pub fn reset() {}
    pub fn release() {}
    pub fn cancel() {
        if let Some(child) = ACTIVE.lock().unwrap_or_else(|e| e.into_inner()).upgrade() {
            let _ = child.lock().unwrap_or_else(|e| e.into_inner()).kill();
        }
    }
    impl Encoder {
        pub fn new(
            path: &Path,
            wav: Option<&Path>,
            s: &VideoSettings,
            p: VideoPlan,
        ) -> Result<Self> {
            let log = tempfile::tempfile()?;
            let mut command = Command::new("ffmpeg");
            command.args([
                "-nostdin",
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "rawvideo",
                "-pixel_format",
                "rgba",
                "-video_size",
                &format!("{}x{}", p.width, p.height),
                "-framerate",
                &p.fps.to_string(),
                "-i",
                "pipe:0",
            ]);
            if let Some(wav) = wav {
                command.arg("-i").arg(wav);
            }
            command.args([
                "-map",
                "0:v:0",
                "-c:v",
                "libx264",
                "-preset",
                "fast",
                "-pix_fmt",
                "yuv420p",
                "-b:v",
                &format!("{}M", s.bitrate_mbps),
                "-threads",
                "2",
            ]);
            if wav.is_some() {
                command.args([
                    "-map",
                    "1:a:0",
                    "-c:a",
                    "aac",
                    "-b:a",
                    &format!("{}k", s.audio_bitrate_kbps),
                ]);
            }
            command
                .args(["-t", &p.duration().to_string(), "-movflags", "+faststart"])
                .arg(path)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::from(log.try_clone()?));
            let mut child = command
                .spawn()
                .context("Install FFmpeg to export MP4 on desktop")?;
            let input = child.stdin.take();
            let child = Arc::new(Mutex::new(child));
            *ACTIVE.lock().unwrap_or_else(|e| e.into_inner()) = Arc::downgrade(&child);
            Ok(Self { child, input, log })
        }
        pub fn frame(&mut self, rgba: &[u8], _index: u64) -> Result<()> {
            self.input
                .as_mut()
                .context("Video encoder is closed")?
                .write_all(rgba)
                .context("Could not send a video frame to FFmpeg")
        }
        pub fn finish(&mut self) -> Result<()> {
            self.input.take();
            let status = loop {
                if let Some(status) = self
                    .child
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .try_wait()?
                {
                    break status;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            };
            use std::io::{Read, Seek};
            self.log.rewind()?;
            let mut message = String::new();
            self.log.read_to_string(&mut message)?;
            ensure!(status.success(), "Video encoder failed: {message}");
            Ok(())
        }
    }
    impl Drop for Encoder {
        fn drop(&mut self) {
            self.input.take();
            let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
            if !matches!(child.try_wait(), Ok(Some(_))) {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}
