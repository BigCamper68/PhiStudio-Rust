//! Bounded asynchronous host I/O. Network waits never occupy the plugin worker.
mod policy;
mod transport;
mod websocket;
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::de::DeserializeOwned;
use std::{
    collections::VecDeque,
    io::{Read, Seek, SeekFrom},
    sync::{
        Weak,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

const MAX_JOBS: usize = 16;
const MAX_OWNER_JOBS: usize = 4;
const MAX_STORED_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone)]
struct Scope {
    call: HostCall,
    registry: Arc<Mutex<Registry>>,
}
impl Scope {
    fn current(&self) -> bool {
        self.registry
            .lock()
            .is_ok_and(|r| r.valid_scope(&self.call.origin, &self.call.required))
            && self.call.invocation().is_none_or(|i| i.is_active())
            && self.call.interaction().is_none_or(|i| i.is_active())
            && self.call.operation().is_none_or(|o| o.is_running())
    }
}

struct Download {
    scope: Scope,
    id: String,
    file: tempfile::NamedTempFile,
    received: AtomicU64,
    total: AtomicU64,
    cancelled: AtomicBool,
    state: Mutex<(NetworkState, Option<String>)>,
    reserved: AtomicU64,
    storage: Arc<AtomicU64>,
}
impl Download {
    fn progress(&self) -> NetworkProgress {
        let (state, error) = self.state.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let total = self.total.load(Ordering::Acquire);
        NetworkProgress {
            id: self.id.clone(),
            state,
            error,
            received: self.received.load(Ordering::Acquire),
            total: (total != u64::MAX).then_some(total),
        }
    }
    fn active(&self) -> bool {
        !self.cancelled.load(Ordering::Acquire) && self.scope.current()
    }
    fn reserve(&self, bytes: u64) -> Result<(), &'static str> {
        let previous = self.storage.fetch_add(bytes, Ordering::AcqRel);
        if previous > MAX_STORED_BYTES || bytes > MAX_STORED_BYTES - previous {
            self.storage.fetch_sub(bytes, Ordering::AcqRel);
            return Err("network_storage_limit");
        }
        self.reserved.fetch_add(bytes, Ordering::Release);
        Ok(())
    }
    fn finish(&self, state: NetworkState, error: Option<String>) {
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = (state, error);
    }
}
impl Drop for Download {
    fn drop(&mut self) {
        self.storage
            .fetch_sub(self.reserved.load(Ordering::Acquire), Ordering::AcqRel);
    }
}

struct Active(Arc<AtomicUsize>);
impl Active {
    fn admit(counter: &Arc<AtomicUsize>) -> Result<Self, &'static str> {
        if counter.fetch_add(1, Ordering::AcqRel) >= MAX_JOBS {
            counter.fetch_sub(1, Ordering::AcqRel);
            return Err("io_busy");
        }
        Ok(Self(counter.clone()))
    }
}
impl Drop for Active {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

type Sender = Weak<SyncSender<(u64, Action)>>;
type JobKey = (String, u64, String);
type Deferred = (
    Option<JobKey>,
    std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
);

pub(super) struct Io {
    runtime: Option<tokio::runtime::Runtime>,
    sender: Sender,
    registry: Arc<Mutex<Registry>>,
    active: Arc<AtomicUsize>,
    stored: Arc<AtomicU64>,
    credentials: Arc<dyn crate::CredentialStore>,
    jobs: BTreeMap<JobKey, Arc<Download>>,
    sockets: BTreeMap<JobKey, Arc<websocket::Socket>>,
    recent: BTreeMap<(String, u64), VecDeque<Instant>>,
    pending: Vec<Deferred>,
}

impl Io {
    pub(super) fn new(
        sender: Sender,
        registry: Arc<Mutex<Registry>>,
        active: Arc<AtomicUsize>,
        credentials: Arc<dyn crate::CredentialStore>,
    ) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .max_blocking_threads(4)
            .thread_name("phistudio-plugin-io")
            .enable_all()
            .build()
            .map_err(|_| "Network executor could not start")?;
        Ok(Self {
            runtime: Some(runtime),
            sender,
            registry,
            active,
            stored: Arc::new(AtomicU64::new(0)),
            credentials: Arc::new(crate::credentials::BoundedCredentialStore::new(credentials)),
            jobs: BTreeMap::new(),
            sockets: BTreeMap::new(),
            recent: BTreeMap::new(),
            pending: Vec::new(),
        })
    }
    /// Publish activation metadata before admitting external side effects. A
    /// failed dispatch discards all I/O it had only staged.
    pub(super) fn flush(&mut self, commit: bool) {
        for (key, task) in self.pending.drain(..) {
            if commit && let Some(runtime) = &self.runtime {
                runtime.spawn(task);
            } else if let Some(key) = key {
                self.jobs.remove(&key);
                self.sockets.remove(&key);
            }
        }
    }
    pub(super) fn prune(&mut self) {
        self.sockets.retain(|_, socket| {
            let current = socket.scope.current();
            if !current {
                socket.cancel();
            }
            current
        });
        self.jobs.retain(|_, job| {
            let current = job.scope.current();
            if !current {
                job.cancelled.store(true, Ordering::Release);
            }
            current
        });
        self.recent.retain(|(plugin, generation), _| {
            self.registry.lock().is_ok_and(|r| {
                r.current(&Origin {
                    plugin: plugin.clone(),
                    generation: *generation,
                })
                .is_some()
            })
        });
    }
    pub(super) fn service(
        &mut self,
        call: HostCall,
        network: NetworkPermissions,
        granted: &BTreeSet<Capability>,
    ) -> Option<ServiceOutcome> {
        self.prune();
        let result = if call.request.service.id == "websocket" {
            self.websocket_service(call, network, granted)
        } else if call.request.service.id == "secrets" {
            self.secret_service(call, granted)
        } else {
            self.network_service(call, network, granted)
        };
        match result {
            Ok(outcome) => outcome,
            Err(code) => Some(ServiceOutcome::error(code, io_message(code))),
        }
    }
    fn network_service(
        &mut self,
        call: HostCall,
        network: NetworkPermissions,
        granted: &BTreeSet<Capability>,
    ) -> Result<Option<ServiceOutcome>, &'static str> {
        if !granted.contains(&Capability::Network) {
            return Err("permission_denied");
        }
        if !call.request.service.version.matches(&Version::new(1, 0, 0)) {
            return Err("unsupported_version");
        }
        let key = |id: &str| {
            (
                call.origin.plugin.clone(),
                call.origin.generation,
                id.to_owned(),
            )
        };
        match call.request.method.as_str() {
            "request" => {
                let request: NetworkRequest = args(&call)?;
                request.validate().map_err(|_| "invalid_arguments")?;
                let url = url::Url::parse(&request.url).map_err(|_| "invalid_arguments")?;
                policy::url_allowed(&url, &network)?;
                if request.credential.is_some() && !granted.contains(&Capability::Secrets) {
                    return Err("permission_denied");
                }
                let id = key(&request.id);
                if self.jobs.contains_key(&id) || self.sockets.contains_key(&id) {
                    return Err("request_exists");
                }
                let owner = (call.origin.plugin.clone(), call.origin.generation);
                if self.jobs.len() + self.sockets.len() >= MAX_JOBS
                    || self
                        .jobs
                        .keys()
                        .chain(self.sockets.keys())
                        .filter(|(p, g, _)| p == &owner.0 && *g == owner.1)
                        .count()
                        >= MAX_OWNER_JOBS
                {
                    return Err("io_busy");
                }
                let recent = self.recent.entry(owner).or_default();
                while recent
                    .front()
                    .is_some_and(|t| t.elapsed() >= Duration::from_secs(60))
                {
                    recent.pop_front();
                }
                if recent.len() >= 60 {
                    return Err("rate_limited");
                }
                let active = Active::admit(&self.active)?;
                let file = tempfile::NamedTempFile::new().map_err(|_| "temporary_file")?;
                let job = Arc::new(Download {
                    scope: Scope {
                        call,
                        registry: self.registry.clone(),
                    },
                    id: request.id.clone(),
                    file,
                    received: AtomicU64::new(0),
                    total: AtomicU64::new(u64::MAX),
                    cancelled: AtomicBool::new(false),
                    state: Mutex::new((NetworkState::Running, None)),
                    reserved: AtomicU64::new(0),
                    storage: self.stored.clone(),
                });
                self.jobs.insert(id.clone(), job.clone());
                recent.push_back(Instant::now());
                let sender = self.sender.clone();
                let credentials = self.credentials.clone();
                self.pending.push((
                    Some(id),
                    Box::pin(async move {
                        let _active = active;
                        let timeout = Duration::from_millis(request.timeout_ms);
                        let result = tokio::time::timeout(timeout, async {
                        let task = transport::request(&request, &network, credentials, &job);
                        tokio::pin!(task);
                        let mut tick = tokio::time::interval(Duration::from_millis(50));
                        loop { tokio::select! {
                            result = &mut task => return result,
                            _ = tick.tick() => if !job.active() { return Err("cancelled"); },
                        }}
                    }).await.unwrap_or(Err("timeout"));
                        let result = if !job.active() {
                            Err("cancelled")
                        } else {
                            result
                        };
                        let outcome = match result {
                            Ok(response) => {
                                job.finish(NetworkState::Ready, None);
                                encode(response)
                            }
                            Err(code) => {
                                job.finish(
                                    if code == "cancelled" {
                                        NetworkState::Cancelled
                                    } else {
                                        NetworkState::Failed
                                    },
                                    Some(code.into()),
                                );
                                ServiceOutcome::error(code, io_message(code))
                            }
                        };
                        post(sender, job.scope.clone(), outcome).await;
                    }),
                ));
                Ok(None)
            }
            "status" | "read" | "cancel" | "close" => {
                let (id, read) = if call.request.method == "read" {
                    let read: NetworkRead = args(&call)?;
                    read.validate().map_err(|_| "invalid_arguments")?;
                    (read.id.clone(), Some(read))
                } else {
                    let id: NetworkId = args(&call)?;
                    (id.id, None)
                };
                if !valid_id(&id) {
                    return Err("invalid_arguments");
                }
                let key = key(&id);
                let job = self.jobs.get(&key).ok_or("unknown_request")?;
                let outcome = match call.request.method.as_str() {
                    "status" => encode(job.progress()),
                    "read" => {
                        let read = read.ok_or("invalid_arguments")?;
                        let received = job.received.load(Ordering::Acquire);
                        if read.offset > received {
                            return Err("invalid_offset");
                        }
                        let count = (received - read.offset).min(read.limit as u64) as usize;
                        let mut file =
                            std::fs::File::open(job.file.path()).map_err(|_| "temporary_file")?;
                        file.seek(SeekFrom::Start(read.offset))
                            .map_err(|_| "temporary_file")?;
                        let mut bytes = vec![0; count];
                        file.read_exact(&mut bytes).map_err(|_| "temporary_file")?;
                        let next_offset = read.offset + count as u64;
                        encode(NetworkChunk {
                            base64: STANDARD.encode(bytes),
                            next_offset,
                            eof: job.progress().state == NetworkState::Ready
                                && next_offset == received,
                        })
                    }
                    "cancel" => {
                        job.cancelled.store(true, Ordering::Release);
                        encode(serde_json::json!({"cancelled":true}))
                    }
                    _ => {
                        job.cancelled.store(true, Ordering::Release);
                        self.jobs.remove(&key);
                        encode(serde_json::json!({"closed":true}))
                    }
                };
                Ok(Some(outcome))
            }
            _ => Err("unknown_method"),
        }
    }

    fn secret_service(
        &mut self,
        mut call: HostCall,
        granted: &BTreeSet<Capability>,
    ) -> Result<Option<ServiceOutcome>, &'static str> {
        if !granted.contains(&Capability::Secrets) {
            return Err("permission_denied");
        }
        if !call.request.service.version.matches(&Version::new(1, 0, 0)) {
            return Err("unsupported_version");
        }
        if !self.credentials.supported() {
            return Err("credentials_unavailable");
        }
        let put = if call.request.method == "put" {
            let secret: SecretWrite = args(&call)?;
            crate::validate_secret(&secret).map_err(|_| "invalid_arguments")?;
            Some(secret)
        } else {
            None
        };
        let key = if let Some(secret) = &put {
            secret.key.clone()
        } else {
            if !matches!(call.request.method.as_str(), "info" | "remove") {
                return Err("unknown_method");
            }
            let key: SecretKey = args(&call)?;
            if !valid_id(&key.key) {
                return Err("invalid_arguments");
            }
            key.key
        };
        let active = Active::admit(&self.active)?;
        let method = call.request.method.clone();
        // Continuations never retain the plaintext put payload.
        call.request.arguments = Value::Null;
        let scope = Scope {
            call,
            registry: self.registry.clone(),
        };
        let sender = self.sender.clone();
        let store = self.credentials.clone();
        self.pending.push((
            None,
            Box::pin(async move {
                let _active = active;
                let operation_scope = scope.clone();
                let result = tokio::task::spawn_blocking(move || {
                    if !operation_scope.current() {
                        return Err("cancelled");
                    }
                    let owner = &operation_scope.call.origin.plugin;
                    match method.as_str() {
                        "put" => {
                            store
                                .write(owner, &put.ok_or("invalid_arguments")?)
                                .map_err(|_| "credentials_unavailable")?;
                            Ok(serde_json::json!({"stored":true}))
                        }
                        "remove" => {
                            store
                                .remove(owner, &key)
                                .map_err(|_| "credentials_unavailable")?;
                            Ok(serde_json::json!({"removed":true}))
                        }
                        _ => {
                            let secret = store
                                .read(owner, &key)
                                .map_err(|_| "credentials_unavailable")?;
                            serde_json::to_value(SecretInfo {
                                key,
                                exists: secret.is_some(),
                                origins: secret.map_or_else(Default::default, |s| s.origins),
                            })
                            .map_err(|_| "encoding")
                        }
                    }
                })
                .await
                .unwrap_or(Err("credentials_unavailable"));
                let outcome = match result {
                    Ok(value) => ServiceOutcome::Success { value },
                    Err(code) => ServiceOutcome::error(code, io_message(code)),
                };
                post(sender, scope, outcome).await;
            }),
        ));
        Ok(None)
    }
}

async fn post(sender: Sender, scope: Scope, outcome: ServiceOutcome) {
    if !scope.current() {
        return;
    }
    let _ = tokio::task::spawn_blocking(move || {
        if scope.current()
            && let Some(sender) = sender.upgrade()
        {
            let _ = sender.send((
                0,
                Action::ServiceReply {
                    call: scope.call,
                    outcome,
                },
            ));
        }
    })
    .await;
}
fn args<T: DeserializeOwned>(call: &HostCall) -> Result<T, &'static str> {
    serde_json::from_value(call.request.arguments.clone()).map_err(|_| "invalid_arguments")
}
fn encode(value: impl serde::Serialize) -> ServiceOutcome {
    match serde_json::to_value(value) {
        Ok(value) => ServiceOutcome::Success { value },
        Err(_) => ServiceOutcome::error("encoding", "Host response could not be encoded"),
    }
}
fn io_message(code: &str) -> &'static str {
    match code {
        "permission_denied" => "The plugin has not been granted this access",
        "origin_denied" => "The server is outside the approved network origins",
        "address_denied" => "The resolved address is outside the approved network scope",
        "cancelled" => "The operation was cancelled",
        "timeout" => "The network operation timed out",
        "io_busy" | "rate_limited" => "The plugin reached its I/O concurrency or request limit",
        "credentials_unavailable" => "Protected credential storage is unavailable or locked",
        "credential_origin" => "The credential is not authorized for this origin",
        "unknown_request" => "The request does not belong to this activation or was closed",
        _ => "The host I/O operation could not be completed",
    }
}
impl Drop for Io {
    fn drop(&mut self) {
        for socket in self.sockets.values() {
            socket.cancel();
        }
        for job in self.jobs.values() {
            job.cancelled.store(true, Ordering::Release);
        }
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(Duration::from_secs(1));
        }
    }
}
