use super::*;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{Notify, mpsc};
use tokio_tungstenite::tungstenite::{
    Message, client::IntoClientRequest, protocol::WebSocketConfig,
};

const MAX_SOCKET_BYTES: u64 = 64 * 1024 * 1024;
const MAX_QUEUED: usize = 16;

struct Data {
    state: WebSocketState,
    received: u64,
    sent: u64,
    messages: VecDeque<WebSocketMessage>,
    error: Option<String>,
}
pub(super) struct Socket {
    pub(super) scope: Scope,
    id: String,
    data: Mutex<Data>,
    in_flight: Mutex<Option<Scope>>,
    cancelled: AtomicBool,
    receiver: AtomicBool,
    notify: Notify,
    outgoing: mpsc::Sender<(Scope, Message)>,
}
struct Receiving(Arc<Socket>);
impl Drop for Receiving {
    fn drop(&mut self) {
        self.0.receiver.store(false, Ordering::Release);
    }
}
impl Socket {
    fn active(&self) -> bool {
        !self.cancelled.load(Ordering::Acquire) && self.scope.current()
    }
    pub(super) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }
    fn status(&self) -> WebSocketStatus {
        let d = self.data.lock().unwrap_or_else(|e| e.into_inner());
        WebSocketStatus {
            id: self.id.clone(),
            state: d.state,
            received: d.received,
            sent: d.sent,
            queued: d.messages.len(),
            error: d.error.clone(),
        }
    }
    fn finish(&self, result: Result<(), &'static str>) {
        let mut d = self.data.lock().unwrap_or_else(|e| e.into_inner());
        d.state = if result.is_ok() || result == Err("cancelled") {
            WebSocketState::Closed
        } else {
            WebSocketState::Failed
        };
        d.error = result.err().map(str::to_owned);
        drop(d);
        self.notify.notify_waiters();
    }
}

impl Io {
    pub(super) fn websocket_service(
        &mut self,
        call: HostCall,
        permissions: NetworkPermissions,
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
            "open" => {
                let request: WebSocketOpen = args(&call)?;
                request.validate().map_err(|_| "invalid_arguments")?;
                let url = url::Url::parse(&request.url).map_err(|_| "invalid_arguments")?;
                policy::url_allowed(&url, &permissions)?;
                if request.credential.is_some() && !granted.contains(&Capability::Secrets) {
                    return Err("permission_denied");
                }
                let id = key(&request.id);
                if self.sockets.contains_key(&id) || self.jobs.contains_key(&id) {
                    return Err("request_exists");
                }
                if self.sockets.len() >= 8
                    || self.jobs.len() + self.sockets.len() >= MAX_JOBS
                    || self
                        .sockets
                        .keys()
                        .filter(|(p, g, _)| p == &id.0 && *g == id.1)
                        .count()
                        >= 2
                    || self
                        .jobs
                        .keys()
                        .chain(self.sockets.keys())
                        .filter(|(p, g, _)| p == &id.0 && *g == id.1)
                        .count()
                        >= MAX_OWNER_JOBS
                {
                    return Err("io_busy");
                }
                let recent = self.recent.entry((id.0.clone(), id.1)).or_default();
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
                recent.push_back(Instant::now());
                let (outgoing, receiver) = mpsc::channel(8);
                let socket = Arc::new(Socket {
                    scope: Scope {
                        call,
                        registry: self.registry.clone(),
                    },
                    id: request.id.clone(),
                    data: Mutex::new(Data {
                        state: WebSocketState::Connecting,
                        received: 0,
                        sent: 0,
                        messages: VecDeque::new(),
                        error: None,
                    }),
                    in_flight: Mutex::new(None),
                    cancelled: AtomicBool::new(false),
                    receiver: AtomicBool::new(false),
                    notify: Notify::new(),
                    outgoing,
                });
                self.sockets.insert(id.clone(), socket.clone());
                let sender = self.sender.clone();
                let credentials = self.credentials.clone();
                self.pending.push((
                    Some(id),
                    Box::pin(async move {
                        let _active = active;
                        let mut receiver = receiver;
                        let result = {
                            let task = run(
                                &socket,
                                request,
                                permissions,
                                credentials,
                                &mut receiver,
                                sender.clone(),
                            );
                            tokio::pin!(task);
                            let mut tick = tokio::time::interval(Duration::from_millis(50));
                            let lifetime = tokio::time::sleep(Duration::from_secs(3600));
                            tokio::pin!(lifetime);
                            loop {
                                tokio::select! {
                                    result=&mut task=>break result,
                                    _=tick.tick()=>if !socket.active() {break Err("cancelled")},
                                    _=&mut lifetime=>break Err("socket_lifetime"),
                                }
                            }
                        };
                        let connecting = socket.status().state == WebSocketState::Connecting;
                        socket.finish(result);
                        let sending = socket
                            .in_flight
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .take();
                        if let Some(scope) = sending {
                            post(
                                sender.clone(),
                                scope,
                                ServiceOutcome::error(
                                    "socket_closed",
                                    "WebSocket connection closed before sending",
                                ),
                            )
                            .await;
                        }
                        while let Ok((scope, _)) = receiver.try_recv() {
                            post(
                                sender.clone(),
                                scope,
                                ServiceOutcome::error(
                                    "socket_closed",
                                    "WebSocket connection closed before sending",
                                ),
                            )
                            .await;
                        }
                        if connecting {
                            post(
                                sender,
                                socket.scope.clone(),
                                ServiceOutcome::error(
                                    result.err().unwrap_or("connection_failed"),
                                    "WebSocket connection could not be opened",
                                ),
                            )
                            .await;
                        }
                    }),
                ));
                Ok(None)
            }
            "status" | "close" => {
                let id: NetworkId = args(&call)?;
                let socket = self.sockets.get(&key(&id.id)).ok_or("unknown_request")?;
                if call.request.method == "close" {
                    socket.cancel();
                    self.sockets.remove(&key(&id.id));
                    Ok(Some(encode(serde_json::json!({"closed":true}))))
                } else {
                    Ok(Some(encode(socket.status())))
                }
            }
            "send" => {
                let request: WebSocketSend = args(&call)?;
                let socket = self
                    .sockets
                    .get(&key(&request.id))
                    .ok_or("unknown_request")?;
                if socket.status().state != WebSocketState::Open || !socket.active() {
                    return Err("socket_closed");
                }
                let message = match request.message {
                    WebSocketMessage::Text { text } => {
                        if text.len() > MAX_WEBSOCKET_MESSAGE_BYTES {
                            return Err("message_too_large");
                        };
                        Message::Text(text.into())
                    }
                    WebSocketMessage::Binary { base64 } => {
                        if base64.len() > MAX_WEBSOCKET_MESSAGE_BYTES.div_ceil(3) * 4 {
                            return Err("message_too_large");
                        };
                        let bytes = STANDARD.decode(base64).map_err(|_| "invalid_arguments")?;
                        if bytes.len() > MAX_WEBSOCKET_MESSAGE_BYTES {
                            return Err("message_too_large");
                        };
                        Message::Binary(bytes.into())
                    }
                };
                socket
                    .outgoing
                    .try_send((
                        Scope {
                            call,
                            registry: self.registry.clone(),
                        },
                        message,
                    ))
                    .map_err(|_| "io_busy")?;
                Ok(None)
            }
            "receive" => {
                let request: WebSocketReceive = args(&call)?;
                request.validate().map_err(|_| "invalid_arguments")?;
                let socket = self
                    .sockets
                    .get(&key(&request.id))
                    .ok_or("unknown_request")?
                    .clone();
                if socket.receiver.swap(true, Ordering::AcqRel) {
                    return Err("receive_pending");
                }
                let active = match Active::admit(&self.active) {
                    Ok(a) => a,
                    Err(e) => {
                        socket.receiver.store(false, Ordering::Release);
                        return Err(e);
                    }
                };
                let scope = Scope {
                    call,
                    registry: self.registry.clone(),
                };
                let sender = self.sender.clone();
                let receiving = Receiving(socket.clone());
                self.pending.push((
                    None,
                    Box::pin(async move {
                        let _active = active;
                        let _receiving = receiving;
                        let wait = async {
                            loop {
                                let notified = socket.notify.notified();
                                let message = socket
                                    .data
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .messages
                                    .pop_front();
                                if message.is_some()
                                    || !socket.active()
                                    || matches!(
                                        socket.status().state,
                                        WebSocketState::Closed | WebSocketState::Failed
                                    )
                                {
                                    return message;
                                }
                                notified.await;
                            }
                        };
                        let message =
                            tokio::time::timeout(Duration::from_millis(request.timeout_ms), wait)
                                .await
                                .unwrap_or(None);
                        post(
                            sender,
                            scope,
                            encode(WebSocketReceived {
                                message,
                                status: socket.status(),
                            }),
                        )
                        .await;
                    }),
                ));
                Ok(None)
            }
            _ => Err("unknown_method"),
        }
    }
}

async fn run(
    socket: &Arc<Socket>,
    request: WebSocketOpen,
    permissions: NetworkPermissions,
    credentials: Arc<dyn crate::CredentialStore>,
    receiver: &mut mpsc::Receiver<(Scope, Message)>,
    sender: Sender,
) -> Result<(), &'static str> {
    let connect = async {
        let url = url::Url::parse(&request.url).map_err(|_| "invalid_arguments")?;
        let addresses = policy::resolve(&url, &permissions).await?;
        let stream = tokio::net::TcpStream::connect(addresses.as_slice())
            .await
            .map_err(|_| "connection_failed")?;
        if !socket.active() {
            return Err("cancelled");
        }
        let mut headers = NetworkRequest::get(&request.id, &request.url);
        headers.headers = request.headers;
        headers.credential = request.credential;
        let headers =
            transport::request_headers(&headers, &socket.scope.call.origin.plugin, credentials)
                .await?;
        let mut outgoing = request
            .url
            .into_client_request()
            .map_err(|_| "invalid_arguments")?;
        outgoing.headers_mut().extend(headers);
        if !request.protocols.is_empty() {
            outgoing.headers_mut().insert(
                "sec-websocket-protocol",
                request
                    .protocols
                    .join(", ")
                    .parse()
                    .map_err(|_| "invalid_arguments")?,
            );
        }
        let config = WebSocketConfig::default()
            .read_buffer_size(4096)
            .write_buffer_size(0)
            .max_write_buffer_size(MAX_WEBSOCKET_MESSAGE_BYTES + 4096)
            .max_message_size(Some(MAX_WEBSOCKET_MESSAGE_BYTES))
            .max_frame_size(Some(MAX_WEBSOCKET_MESSAGE_BYTES));
        let (stream, response) =
            tokio_tungstenite::client_async_tls_with_config(outgoing, stream, Some(config), None)
                .await
                .map_err(|_| "connection_failed")?;
        let protocol = response
            .headers()
            .get("sec-websocket-protocol")
            .map(|v| v.to_str().map(str::to_owned))
            .transpose()
            .map_err(|_| "connection_failed")?;
        if protocol
            .as_ref()
            .is_some_and(|p| !request.protocols.contains(p))
        {
            return Err("invalid_subprotocol");
        }
        Ok((stream, protocol))
    };
    let (mut stream, protocol) = tokio::time::timeout(Duration::from_secs(15), connect)
        .await
        .map_err(|_| "timeout")??;
    socket.data.lock().unwrap_or_else(|e| e.into_inner()).state = WebSocketState::Open;
    post(
        sender.clone(),
        socket.scope.clone(),
        encode(serde_json::json!({"id":socket.id,"protocol":protocol})),
    )
    .await;
    let mut recent = VecDeque::new();
    loop {
        if !socket.active() {
            return Err("cancelled");
        }
        tokio::select! {
            next=stream.next()=> {
                let Some(message)=next else {return Ok(())};
                let message=message.map_err(|_|"connection_failed")?;
                if message.is_close() {return Ok(())}
                if message.is_ping() {tokio::time::timeout(Duration::from_secs(5),stream.flush()).await.map_err(|_|"timeout")?.map_err(|_|"connection_failed")?;}
                let size=message.len() as u64;
                let message=match message {Message::Text(text)=>Some(WebSocketMessage::Text {text:text.to_string()}),Message::Binary(bytes)=>Some(WebSocketMessage::Binary {base64:STANDARD.encode(bytes)}),_=>None};
                record_rate(&mut recent)?;
                let mut data=socket.data.lock().unwrap_or_else(|e|e.into_inner());
                data.received=data.received.saturating_add(size);
                if data.received>MAX_SOCKET_BYTES {return Err("socket_byte_limit")}
                if let Some(message)=message {if data.messages.len()>=MAX_QUEUED {return Err("socket_queue_full")};data.messages.push_back(message);}
                drop(data);socket.notify.notify_waiters();
            },
            outgoing=receiver.recv()=> {
                let Some((scope,message))=outgoing else {return Ok(())};
                if !scope.current() {continue}
                *socket.in_flight.lock().unwrap_or_else(|e|e.into_inner())=Some(scope.clone());
                record_rate(&mut recent)?;
                let size=message.len() as u64;
                {let mut d=socket.data.lock().unwrap_or_else(|e|e.into_inner());d.sent=d.sent.saturating_add(size);if d.sent>MAX_SOCKET_BYTES {return Err("socket_byte_limit")}}
                let sent=tokio::time::timeout(Duration::from_secs(5),stream.send(message)).await.map_err(|_|"timeout").and_then(|r|r.map_err(|_|"connection_failed"));
                let sending=socket.in_flight.lock().unwrap_or_else(|e|e.into_inner()).take();
                if let Some(scope)=sending {
                    let outcome=match sent {Ok(())=>encode(serde_json::json!({"sent":size})),Err(code)=>ServiceOutcome::error(code,"WebSocket send failed")};
                    post(sender.clone(),scope,outcome).await;
                }
                sent?;
            },
        }
    }
}
fn record_rate(recent: &mut VecDeque<Instant>) -> Result<(), &'static str> {
    while recent
        .front()
        .is_some_and(|t| t.elapsed() >= Duration::from_secs(60))
    {
        recent.pop_front();
    }
    if recent.len() >= 600 {
        return Err("rate_limited");
    }
    recent.push_back(Instant::now());
    Ok(())
}
