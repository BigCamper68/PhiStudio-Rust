use super::*;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use tokio::io::AsyncWriteExt as _;

fn headers(request: &NetworkRequest) -> Result<HeaderMap, &'static str> {
    let mut result = HeaderMap::new();
    for (key, value) in &request.headers {
        let key = HeaderName::from_bytes(key.as_bytes()).map_err(|_| "invalid_header")?;
        if key.as_str().starts_with("sec-websocket-")
            || matches!(
                key.as_str(),
                "host"
                    | "connection"
                    | "upgrade"
                    | "proxy-authorization"
                    | "proxy-connection"
                    | "content-length"
                    | "transfer-encoding"
                    | "te"
                    | "trailer"
                    | "cookie"
                    | "accept-encoding"
            )
        {
            return Err("invalid_header");
        }
        result.insert(
            key,
            HeaderValue::from_str(value).map_err(|_| "invalid_header")?,
        );
    }
    result.insert(
        reqwest::header::ACCEPT_ENCODING,
        HeaderValue::from_static("identity"),
    );
    Ok(result)
}

pub(super) async fn request_headers(
    request: &NetworkRequest,
    owner: &str,
    credentials: Arc<dyn crate::CredentialStore>,
) -> Result<HeaderMap, &'static str> {
    let url = url::Url::parse(&request.url).map_err(|_| "invalid_arguments")?;
    let mut headers = headers(request)?;
    if let Some(reference) = &request.credential {
        if !credentials.supported() {
            return Err("credentials_unavailable");
        }
        let owner = owner.to_owned();
        let key = reference.key.clone();
        let secret = tokio::task::spawn_blocking(move || credentials.read(&owner, &key))
            .await
            .map_err(|_| "credentials_unavailable")?
            .map_err(|_| "credentials_unavailable")?
            .ok_or("credential_missing")?;
        if !secret.origins.contains(&url.origin().ascii_serialization()) {
            return Err("credential_origin");
        }
        let name =
            HeaderName::from_bytes(reference.header.as_bytes()).map_err(|_| "invalid_header")?;
        if name.as_str().starts_with("sec-websocket-")
            || matches!(
                name.as_str(),
                "host"
                    | "connection"
                    | "upgrade"
                    | "proxy-authorization"
                    | "proxy-connection"
                    | "content-length"
                    | "transfer-encoding"
                    | "te"
                    | "trailer"
                    | "cookie"
                    | "accept-encoding"
            )
        {
            return Err("invalid_header");
        }
        let mut value = HeaderValue::from_str(&format!("{}{}", reference.prefix, secret.value))
            .map_err(|_| "invalid_header")?;
        value.set_sensitive(true);
        headers.insert(name, value);
    }
    Ok(headers)
}

pub(super) async fn request(
    request: &NetworkRequest,
    permissions: &NetworkPermissions,
    credentials: Arc<dyn crate::CredentialStore>,
    job: &Arc<Download>,
) -> Result<NetworkResponse, &'static str> {
    let mut url = url::Url::parse(&request.url).map_err(|_| "invalid_arguments")?;
    let mut headers = request_headers(request, &job.scope.call.origin.plugin, credentials).await?;
    let body = request
        .body_base64
        .as_ref()
        .map(|value| STANDARD.decode(value).map_err(|_| "invalid_arguments"))
        .transpose()?;
    for redirect in 0..=request.redirects {
        if !job.active() {
            return Err("cancelled");
        }
        let addresses = policy::resolve(&url, permissions).await?;
        if !job.active() {
            return Err("cancelled");
        }
        let host = url.host_str().ok_or("invalid_arguments")?;
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .no_zstd()
            .resolve_to_addrs(host, &addresses)
            .connect_timeout(Duration::from_secs(10))
            .user_agent(concat!("PhiStudio-Rust/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| "http_client")?;
        let method = reqwest::Method::from_bytes(request.method.as_bytes())
            .map_err(|_| "invalid_arguments")?;
        let mut outgoing = client.request(method, url.clone()).headers(headers.clone());
        if let Some(body) = &body {
            outgoing = outgoing.body(body.clone());
        }
        let mut response = outgoing.send().await.map_err(|error| {
            if error.is_timeout() {
                "timeout"
            } else {
                "connection_failed"
            }
        })?;
        let remote = response.remote_addr().ok_or("address_denied")?;
        if !addresses.iter().any(|address| address.ip() == remote.ip()) {
            return Err("address_denied");
        }
        let status = response.status().as_u16();
        if matches!(status, 301 | 302 | 303 | 307 | 308)
            && matches!(request.method.as_str(), "GET" | "HEAD")
            && body.is_none()
            && request.redirects > 0
            && let Some(location) = response.headers().get(reqwest::header::LOCATION)
        {
            if redirect == request.redirects {
                return Err("redirect_limit");
            }
            let next = url
                .join(location.to_str().map_err(|_| "invalid_redirect")?)
                .map_err(|_| "invalid_redirect")?;
            policy::url_allowed(&next, permissions)?;
            if next.origin() != url.origin() {
                // Every caller-supplied header may contain a credential. Never
                // forward any of them to a different approved server.
                headers = HeaderMap::new();
                headers.insert(
                    reqwest::header::ACCEPT_ENCODING,
                    HeaderValue::from_static("identity"),
                );
            }
            url = next;
            continue;
        }
        let total = if request.method == "HEAD" {
            Some(0)
        } else {
            response.content_length()
        };
        if total.is_some_and(|n| n > request.max_bytes) {
            return Err("response_too_large");
        }
        job.total
            .store(total.unwrap_or(u64::MAX), Ordering::Release);
        let mut returned_headers = BTreeMap::<String, Vec<String>>::new();
        let mut header_bytes = 0usize;
        for (name, value) in response.headers() {
            header_bytes = header_bytes
                .saturating_add(name.as_str().len())
                .saturating_add(value.len());
            if header_bytes > 64 * 1024
                || (!returned_headers.contains_key(name.as_str()) && returned_headers.len() >= 128)
            {
                return Err("response_headers_too_large");
            }
            returned_headers
                .entry(name.as_str().into())
                .or_default()
                .push(
                    value
                        .to_str()
                        .map_err(|_| "response_header_encoding")?
                        .into(),
                );
        }
        let mut output = tokio::fs::OpenOptions::new()
            .write(true)
            .open(job.file.path())
            .await
            .map_err(|_| "temporary_file")?;
        while let Some(chunk) = response.chunk().await.map_err(|_| "connection_failed")? {
            if !job.active() {
                return Err("cancelled");
            }
            let received = job.received.load(Ordering::Acquire);
            let next = received
                .checked_add(chunk.len() as u64)
                .ok_or("response_too_large")?;
            if next > request.max_bytes {
                return Err("response_too_large");
            }
            job.reserve(chunk.len() as u64)?;
            output
                .write_all(&chunk)
                .await
                .map_err(|_| "temporary_file")?;
            output.flush().await.map_err(|_| "temporary_file")?;
            job.received.store(next, Ordering::Release);
        }
        output.flush().await.map_err(|_| "temporary_file")?;
        if !job.active() {
            return Err("cancelled");
        }
        let bytes = job.received.load(Ordering::Acquire);
        let inline = if bytes <= MAX_NETWORK_INLINE_BYTES as u64 {
            Some(
                STANDARD.encode(
                    tokio::fs::read(job.file.path())
                        .await
                        .map_err(|_| "temporary_file")?,
                ),
            )
        } else {
            None
        };
        return Ok(NetworkResponse {
            id: request.id.clone(),
            status,
            headers: returned_headers,
            bytes,
            body_base64: inline,
        });
    }
    Err("redirect_limit")
}
