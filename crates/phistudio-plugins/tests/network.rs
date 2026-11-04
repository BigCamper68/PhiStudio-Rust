use phistudio_plugins::*;
use phistudio_sdk::*;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

struct Server {
    origin: String,
    received: mpsc::Receiver<String>,
    reply: mpsc::SyncSender<Vec<u8>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (sent, received) = mpsc::channel();
        let (reply, input) = mpsc::sync_channel::<Vec<u8>>(1);
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                if stopped.load(Ordering::Acquire) || Instant::now() > deadline {
                    return;
                }
                if let Ok((stream, _)) = listener.accept() {
                    break stream;
                }
                std::thread::sleep(Duration::from_millis(2));
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut headers = Vec::new();
            while headers.len() < 64 * 1024 && !headers.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                if stream.read_exact(&mut byte).is_err() {
                    return;
                }
                headers.push(byte[0]);
            }
            let _ = sent.send(String::from_utf8_lossy(&headers).into_owned());
            if let Ok(response) = input.recv_timeout(Duration::from_secs(10)) {
                let _ = stream.write_all(&response);
            }
        });
        Self {
            origin,
            received,
            reply,
            stop,
            thread: Some(thread),
        }
    }
    fn connected(&self) -> String {
        self.received.recv_timeout(Duration::from_secs(5)).unwrap()
    }
    fn body(&self, body: &[u8]) {
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.extend_from_slice(body);
        self.reply.send(response).unwrap();
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.reply.try_send(Vec::new());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Default)]
struct Secrets(Mutex<std::collections::BTreeMap<(String, String), SecretWrite>>);
impl CredentialStore for Secrets {
    fn supported(&self) -> bool {
        true
    }
    fn read(&self, plugin: &str, key: &str) -> Result<Option<SecretWrite>, String> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get(&(plugin.into(), key.into()))
            .cloned())
    }
    fn write(&self, plugin: &str, secret: &SecretWrite) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .insert((plugin.into(), secret.key.clone()), secret.clone());
        Ok(())
    }
    fn remove(&self, plugin: &str, key: &str) -> Result<(), String> {
        self.0.lock().unwrap().remove(&(plugin.into(), key.into()));
        Ok(())
    }
}
type Replies = Arc<Mutex<Vec<(String, ServiceReply)>>>;
struct Peer {
    id: String,
    replies: Replies,
    activation: Option<ServiceCall>,
}
impl Plugin for Peer {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => Response {
                contributions: vec![Contribution::Command {
                    id: "call".into(),
                    title: "Call".into(),
                    shortcut: None,
                    snapshot: false,
                }],
                effects: self
                    .activation
                    .take()
                    .map(|call| Effect::Call { call })
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
            Request::Command { arguments, .. } => Response {
                effects: vec![Effect::Call {
                    call: serde_json::from_value(arguments.clone()).unwrap(),
                }],
                ..Default::default()
            },
            Request::ServiceResult { reply, .. } => {
                self.replies
                    .lock()
                    .unwrap()
                    .push((self.id.clone(), reply.clone()));
                Response::default()
            }
            _ => Response::default(),
        })
    }
}
struct Observer(Arc<Mutex<Vec<Value>>>);
impl Plugin for Observer {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => Response {
                contributions: vec![Contribution::Hook {
                    point: SERVICE_BEFORE_HOOK.into(),
                    priority: 0,
                }],
                ..Default::default()
            },
            Request::Hook { payload, .. } => {
                self.0.lock().unwrap().push(payload.clone());
                Response::default()
            }
            _ => Response::default(),
        })
    }
}
struct Fixture {
    runtime: Runtime,
    replies: Replies,
    observed: Arc<Mutex<Vec<Value>>>,
}
impl Fixture {
    fn new(origins: Vec<String>, activation: Option<ServiceCall>) -> Self {
        let replies: Replies = Default::default();
        let observed = Arc::new(Mutex::new(Vec::new()));
        let mut builtins = vec![];
        for id in ["test.network", "test.other"] {
            let manifest:Manifest=serde_json::from_value(json!({"api":1,"id":id,"name":id,"version":"1.0.0","backend":{"kind":"builtin"},
                "capabilities":["services","network","secrets"],"network":{"origins":origins,"local_network":true}})).unwrap();
            let replies = replies.clone();
            let id = id.to_owned();
            let activation = if id == "test.network" {
                activation.clone()
            } else {
                None
            };
            builtins.push(Builtin {
                manifest,
                enabled: true,
                create: Arc::new(move || {
                    Box::new(Peer {
                        id: id.clone(),
                        replies: replies.clone(),
                        activation: activation.clone(),
                    })
                }),
            });
        }
        let observer:Manifest=serde_json::from_value(json!({"api":1,"id":"aaa.observer","name":"Observer","version":"1.0.0","backend":{"kind":"builtin"},"capabilities":["hooks"]})).unwrap();
        let observed_copy = observed.clone();
        builtins.push(Builtin {
            manifest: observer,
            enabled: true,
            create: Arc::new(move || Box::new(Observer(observed_copy.clone()))),
        });
        let runtime = Runtime::spawn_with_config(
            None,
            builtins,
            RuntimeConfig {
                credentials: Arc::new(Secrets::default()),
                ..Default::default()
            },
        )
        .unwrap();
        let mut this = Self {
            runtime,
            replies,
            observed,
        };
        this.wait(0);
        this
    }
    fn wait(&mut self, id: u64) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(done) = self.runtime.poll().unwrap() {
                done.result.unwrap();
                if done.id == id {
                    return;
                }
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    fn call(&mut self, owner: &str, id: &str, service: &str, method: &str, arguments: Value) {
        let call = service_call(id, service, method, arguments);
        let id = self
            .runtime
            .submit(Action::Command {
                plugin: owner.into(),
                id: "call".into(),
                arguments: serde_json::to_value(call).unwrap(),
                context: EditorContext::default(),
            })
            .unwrap();
        self.wait(id);
    }
    fn reply(&mut self, owner: &str, id: &str) -> ServiceOutcome {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let index = self
                .replies
                .lock()
                .unwrap()
                .iter()
                .position(|(p, r)| p == owner && r.request_id == id);
            if let Some(index) = index {
                return self.replies.lock().unwrap().remove(index).1.outcome;
            }
            while let Some(done) = self.runtime.poll().unwrap() {
                done.result.unwrap();
            }
            assert!(Instant::now() < deadline, "Missing private reply {id}");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.runtime.shutdown();
    }
}
fn service_call(id: &str, service: &str, method: &str, arguments: Value) -> ServiceCall {
    ServiceCall {
        request_id: id.into(),
        service: ServiceRef {
            provider: "host".into(),
            id: service.into(),
            version: "^1".parse().unwrap(),
        },
        method: method.into(),
        arguments,
    }
}
fn success(outcome: ServiceOutcome) -> Value {
    let ServiceOutcome::Success { value } = outcome else {
        panic!("{outcome:?}")
    };
    value
}
fn error(outcome: ServiceOutcome, expected: &str) {
    assert!(matches!(outcome,ServiceOutcome::Error {code,..} if code==expected));
}

#[test]
fn untrusted_tls_is_rejected_before_sending_http_or_credentials() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("https://{}", listener.local_addr().unwrap());
    let certificate = rustls::pki_types::CertificateDer::from(
        include_bytes!("fixtures/tls/untrusted-cert.der").to_vec(),
    );
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(rustls::pki_types::PrivatePkcs8KeyDer::from(
        include_bytes!("fixtures/tls/test-only-key.der").to_vec(),
    ));
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(vec![certificate], key)
    .unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut connection = rustls::ServerConnection::new(Arc::new(config)).unwrap();
        let result = connection.complete_io(&mut stream);
        assert!(result.is_err());
        let mut plaintext = vec![];
        let _ = connection.reader().read_to_end(&mut plaintext);
        assert!(plaintext.is_empty());
    });
    let mut f = Fixture::new(vec![origin.clone()], None);
    let mut request = NetworkRequest::get("tls", format!("{origin}/?private-query-canary"));
    request
        .headers
        .insert("authorization".into(), "secret-header-canary".into());
    f.call("test.network", "tls", "network", "request", json!(request));
    let reply = f.reply("test.network", "tls");
    assert!(!format!("{reply:?}").contains("canary"));
    error(reply, "connection_failed");
    server.join().unwrap();
}

#[test]
fn redirects_revalidate_origins_and_strip_all_headers_between_servers() {
    let first = Server::new();
    let second = Server::new();
    let mut f = Fixture::new(vec![first.origin.clone(), second.origin.clone()], None);
    let mut request = NetworkRequest::get("redirect", format!("{}/start", first.origin));
    request
        .headers
        .insert("x-private".into(), "header-canary".into());
    f.call(
        "test.network",
        "redirect",
        "network",
        "request",
        json!(request),
    );
    assert!(first.connected().contains("header-canary"));
    first.reply.send(format!("HTTP/1.1 302 Found\r\nLocation: {}/target\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",second.origin).into_bytes()).unwrap();
    let next = second.connected();
    assert!(!next.contains("header-canary") && !next.contains("x-private"));
    second.body(b"redirected");
    assert_eq!(success(f.reply("test.network", "redirect"))["status"], 200);
    let denied = Server::new();
    let request = NetworkRequest::get("denied", format!("{}/start", denied.origin));
    f.call(
        "test.network",
        "denied",
        "network",
        "request",
        json!(request),
    );
    error(f.reply("test.network", "denied"), "origin_denied");
    assert!(f.observed.lock().unwrap().is_empty());
    let start = Server::new();
    let mut restricted = Fixture::new(vec![start.origin.clone()], None);
    restricted.call(
        "test.network",
        "hop",
        "network",
        "request",
        json!(NetworkRequest::get(
            "hop",
            format!("{}/start", start.origin)
        )),
    );
    start.connected();
    start.reply.send(format!("HTTP/1.1 302 Found\r\nLocation: {}/private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",denied.origin).into_bytes()).unwrap();
    error(restricted.reply("test.network", "hop"), "origin_denied");
    assert!(
        denied.received.try_recv().is_err(),
        "Unapproved redirect must not connect"
    );
}

#[test]
fn websocket_echo_uses_private_async_owned_channels_and_revokes_on_disable() {
    use tokio_tungstenite::tungstenite::{Message, accept};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("ws://{}", listener.local_addr().unwrap());
    let (ended, done) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut socket = accept(stream).unwrap();
        let message = socket.read().unwrap();
        assert_eq!(message, Message::Text("echo canary".into()));
        socket.send(message).unwrap();
        // Disable drops the transport; it need not wait for a closing handshake.
        let result = socket.read();
        assert!(result.is_err() || result.unwrap().is_close());
        ended.send(()).unwrap();
    });
    let activation = service_call(
        "open",
        "websocket",
        "open",
        json!({"id":"socket","url":format!("{origin}/echo")}),
    );
    let mut f = Fixture::new(vec![origin], Some(activation));
    assert_eq!(success(f.reply("test.network", "open"))["id"], "socket");
    f.call(
        "test.other",
        "foreign.socket",
        "websocket",
        "receive",
        json!({"id":"socket","timeout_ms":0}),
    );
    error(f.reply("test.other", "foreign.socket"), "unknown_request");
    f.call(
        "test.network",
        "receive",
        "websocket",
        "receive",
        json!({"id":"socket","timeout_ms":5000}),
    );
    f.call(
        "test.network",
        "send",
        "websocket",
        "send",
        json!({"id":"socket","message":{"kind":"text","text":"echo canary"}}),
    );
    assert_eq!(success(f.reply("test.network", "send"))["sent"], 11);
    let received = success(f.reply("test.network", "receive"));
    assert_eq!(received["message"]["text"], "echo canary");
    f.call("test.network","huge","websocket","send",json!({"id":"socket","message":{"kind":"text","text":"x".repeat(MAX_WEBSOCKET_MESSAGE_BYTES+1)}}));
    error(f.reply("test.network", "huge"), "message_too_large");
    let id = f
        .runtime
        .submit(Action::Disable("test.network".into()))
        .unwrap();
    f.wait(id);
    done.recv_timeout(Duration::from_secs(2)).unwrap();
    server.join().unwrap();
    assert!(f.observed.lock().unwrap().is_empty());
}

#[test]
fn download_streams_to_owned_handles_and_activation_can_start_network_work() {
    let server = Server::new();
    let request = NetworkRequest::get("download", format!("{}/file", server.origin));
    let activation = service_call("fetch", "network", "request", json!(request));
    let mut f = Fixture::new(vec![server.origin.clone()], Some(activation));
    server.connected();
    server.body(b"hello");
    let response: NetworkResponse =
        serde_json::from_value(success(f.reply("test.network", "fetch"))).unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.inline_bytes().unwrap().unwrap(), b"hello");
    f.call(
        "test.other",
        "foreign",
        "network",
        "read",
        json!({"id":"download","offset":0,"limit":5}),
    );
    error(f.reply("test.other", "foreign"), "unknown_request");
    f.call(
        "test.network",
        "read",
        "network",
        "read",
        json!({"id":"download","offset":1,"limit":3}),
    );
    let chunk: NetworkChunk =
        serde_json::from_value(success(f.reply("test.network", "read"))).unwrap();
    assert_eq!(chunk.base64, "ZWxs");
    assert!(!chunk.eof);
    f.call(
        "test.network",
        "close",
        "network",
        "close",
        json!({"id":"download"}),
    );
    success(f.reply("test.network", "close"));
    f.call(
        "test.network",
        "missing",
        "network",
        "status",
        json!({"id":"download"}),
    );
    error(f.reply("test.network", "missing"), "unknown_request");
}

#[test]
fn slow_network_does_not_block_other_callbacks_and_cancel_returns_a_targeted_error() {
    let server = Server::new();
    let mut f = Fixture::new(vec![server.origin.clone()], None);
    f.call(
        "test.network",
        "fetch",
        "network",
        "request",
        json!(NetworkRequest::get(
            "slow",
            format!("{}/slow", server.origin)
        )),
    );
    server.connected();
    f.call("test.other", "features", "features", "get", Value::Null);
    success(f.reply("test.other", "features"));
    f.call(
        "test.network",
        "status",
        "network",
        "status",
        json!({"id":"slow"}),
    );
    let progress: NetworkProgress =
        serde_json::from_value(success(f.reply("test.network", "status"))).unwrap();
    assert_eq!(progress.state, NetworkState::Running);
    f.call(
        "test.network",
        "cancel",
        "network",
        "cancel",
        json!({"id":"slow"}),
    );
    success(f.reply("test.network", "cancel"));
    error(f.reply("test.network", "fetch"), "cancelled");
    server.body(b"late");
}

#[test]
fn secrets_are_owner_scoped_opaque_and_hidden_from_generic_service_hooks() {
    let server = Server::new();
    let mut f = Fixture::new(vec![server.origin.clone()], None);
    f.call(
        "test.network",
        "store",
        "secrets",
        "put",
        json!({"key":"token","value":"canary-secret","origins":[server.origin]}),
    );
    success(f.reply("test.network", "store"));
    f.call(
        "test.other",
        "foreign",
        "secrets",
        "info",
        json!({"key":"token"}),
    );
    assert_eq!(success(f.reply("test.other", "foreign"))["exists"], false);
    f.call(
        "test.network",
        "info",
        "secrets",
        "info",
        json!({"key":"token"}),
    );
    let info = success(f.reply("test.network", "info"));
    assert_eq!(info["exists"], true);
    assert!(!info.to_string().contains("canary-secret"));
    let mut request = NetworkRequest::get("authenticated", format!("{}/api", server.origin));
    request.credential = Some(NetworkCredential {
        key: "token".into(),
        header: "authorization".into(),
        prefix: "Bearer ".into(),
    });
    f.call(
        "test.network",
        "fetch",
        "network",
        "request",
        json!(request),
    );
    assert!(
        server
            .connected()
            .to_lowercase()
            .contains("authorization: bearer canary-secret")
    );
    server.body(b"ok");
    success(f.reply("test.network", "fetch"));
    assert!(f.observed.lock().unwrap().is_empty());
}

#[test]
fn response_limits_and_reserved_addresses_are_enforced_without_exposing_urls() {
    let server = Server::new();
    let mut f = Fixture::new(
        vec![server.origin.clone(), "https://169.254.169.254".into()],
        None,
    );
    let mut request =
        NetworkRequest::get("limited", format!("{}/private?token=canary", server.origin));
    request.max_bytes = 3;
    f.call(
        "test.network",
        "fetch",
        "network",
        "request",
        json!(request),
    );
    server.connected();
    server.body(b"too large");
    let outcome = f.reply("test.network", "fetch");
    assert!(!format!("{outcome:?}").contains("canary"));
    error(outcome, "response_too_large");
    f.call(
        "test.network",
        "metadata",
        "network",
        "request",
        json!(NetworkRequest::get(
            "metadata",
            "https://169.254.169.254/latest/meta-data/"
        )),
    );
    error(f.reply("test.network", "metadata"), "address_denied");
}

#[test]
fn revocation_during_io_discards_late_callbacks_and_shutdown_does_not_wait_for_the_server() {
    let server = Server::new();
    let mut f = Fixture::new(vec![server.origin.clone()], None);
    f.call(
        "test.network",
        "fetch",
        "network",
        "request",
        json!(NetworkRequest::get("download", server.origin.clone())),
    );
    server.connected();
    let origin = f
        .runtime
        .registry()
        .unwrap()
        .plugins
        .iter()
        .find(|p| p.manifest.id == "test.network")
        .unwrap()
        .origin
        .clone()
        .unwrap();
    let id = f
        .runtime
        .submit(Action::Disable("test.network".into()))
        .unwrap();
    assert!(!f.runtime.is_current(&origin));
    f.wait(id);
    std::thread::sleep(Duration::from_millis(120));
    while let Some(done) = f.runtime.poll().unwrap() {
        done.result.unwrap();
    }
    assert!(
        !f.replies
            .lock()
            .unwrap()
            .iter()
            .any(|(p, r)| p == "test.network" && r.request_id == "fetch")
    );
    let started = Instant::now();
    f.runtime.shutdown().unwrap();
    assert!(started.elapsed() < Duration::from_secs(3));
    server.body(b"late");
}
