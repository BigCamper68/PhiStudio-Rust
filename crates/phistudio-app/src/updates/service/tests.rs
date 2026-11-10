use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

fn raw(tag: &str) -> serde_json::Value {
    serde_json::json!({"tag_name":tag,"html_url":format!("{RELEASES}/tag/{tag}"),"draft":false,"prerelease":false})
}
#[test]
fn semantic_versions_handle_double_digits_prereleases_and_build_metadata() {
    for (installed, latest, expected) in [
        ("1.9.0", "v1.10.0", true),
        ("1.32.0", "1.32.0", false),
        ("1.33.0", "v1.32.0", false),
        ("1.32.0-rc.2", "v1.32.0", true),
        ("1.32.0+debug", "v1.32.0+release", false),
        ("1.32.9", "v1.32.10", true),
    ] {
        let release = parse(&serde_json::to_vec(&raw(latest)).unwrap()).unwrap();
        assert_eq!(
            newer(&release, installed).unwrap(),
            expected,
            "{installed} / {latest}"
        );
    }
}
#[test]
fn drafts_prereleases_bad_tags_and_untrusted_links_never_offer_an_update() {
    for key in ["draft", "prerelease"] {
        let mut value = raw("v2.0.0");
        value[key] = true.into();
        assert_eq!(
            parse(&serde_json::to_vec(&value).unwrap()),
            Err(CheckError::NoRelease)
        );
    }
    assert_eq!(
        parse(&serde_json::to_vec(&raw("v2.0.0-beta.1")).unwrap()),
        Err(CheckError::NoRelease)
    );
    assert_eq!(
        parse(&serde_json::to_vec(&raw("nightly")).unwrap()),
        Err(CheckError::InvalidResponse)
    );
    for link in [
        "https://example.com/update",
        "http://github.com/BigCamper68/PhiStudio-Rust/releases/tag/v2.0.0",
        "https://github.com/other/project/releases/tag/v2.0.0",
        "https://github.com.evil.test/BigCamper68/PhiStudio-Rust/releases/tag/v2.0.0",
    ] {
        let mut value = raw("v2.0.0");
        value["html_url"] = link.into();
        assert_eq!(
            parse(&serde_json::to_vec(&value).unwrap()),
            Err(CheckError::InvalidResponse)
        );
    }
}
fn response(status: u16, headers: &str, body: &[u8]) -> Result<Release, CheckError> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/latest", listener.local_addr().unwrap());
    let bytes = format!("HTTP/1.1 {status} Test\r\nConnection: close\r\n{headers}\r\n")
        .into_bytes()
        .into_iter()
        .chain(body.iter().copied())
        .collect::<Vec<_>>();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut b = [0];
            if socket.read(&mut b).unwrap() == 0 {
                break;
            }
            request.push(b[0]);
            assert!(request.len() < 16384);
        }
        let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
        assert!(request.contains("user-agent: phistudio-rust/"));
        assert!(request.contains("accept: application/vnd.github+json"));
        assert!(!request.contains("authorization:"));
        let _ = socket.write_all(&bytes);
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = runtime.block_on(fetch(&endpoint));
    server.join().unwrap();
    result
}
#[test]
fn real_http_transport_reports_errors_and_reads_a_stable_release() {
    let body = serde_json::to_vec(&raw("v99.0.0")).unwrap();
    assert_eq!(
        response(200, &format!("Content-Length: {}\r\n", body.len()), &body)
            .unwrap()
            .version,
        Version::new(99, 0, 0)
    );
    for (status, expected) in [
        (404, CheckError::NoRelease),
        (403, CheckError::RateLimited),
        (429, CheckError::RateLimited),
        (503, CheckError::Server),
        (302, CheckError::Server),
    ] {
        assert_eq!(
            response(status, "Content-Length: 0\r\n", &[]),
            Err(expected)
        );
    }
    assert_eq!(
        response(200, "Content-Length: 4\r\n", b"nope"),
        Err(CheckError::InvalidResponse)
    );
}
#[test]
fn oversized_http_payloads_are_rejected_with_or_without_content_length() {
    assert_eq!(
        response(200, &format!("Content-Length: {}\r\n", MAX_BYTES + 1), &[]),
        Err(CheckError::InvalidResponse)
    );
    let body = vec![b' '; MAX_BYTES + 1];
    assert_eq!(response(200, "", &body), Err(CheckError::InvalidResponse));
}

#[test]
fn release_names_and_notes_are_preserved_and_preview_notes_can_be_read() {
    let mut value = raw("v1.32.0");
    value["name"] = "PhiStudio Rust release title".into();
    value["body"] = "## Changes\n- Editor improvements\n\nOriginal release text.".into();
    let parsed = parse(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(parsed.name, value["name"].as_str().unwrap());
    assert_eq!(parsed.body, value["body"].as_str().unwrap());
    value["tag_name"] = "v1.33.0-rc.1".into();
    value["prerelease"] = true.into();
    let bytes = serde_json::to_vec(&value).unwrap();
    assert_eq!(parse(&bytes), Err(CheckError::NoRelease));
    assert!(parse_release(&bytes, true).is_ok());
    value["body"] = serde_json::Value::Null;
    assert!(
        parse_release(&serde_json::to_vec(&value).unwrap(), true)
            .unwrap()
            .body
            .is_empty()
    );
}
#[test]
fn installed_notes_use_the_exact_version_and_try_an_unprefixed_tag_after_404() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/releases/tags", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        for (path, status, body) in [
            ("/releases/tags/v1.32.0", 404, Vec::new()),
            (
                "/releases/tags/1.32.0",
                200,
                serde_json::to_vec(&raw("1.32.0")).unwrap(),
            ),
        ] {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                assert_ne!(socket.read(&mut byte).unwrap(), 0);
                request.push(byte[0]);
            }
            assert!(
                String::from_utf8(request)
                    .unwrap()
                    .starts_with(&format!("GET {path} "))
            );
            write!(
                socket,
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            socket.write_all(&body).unwrap();
        }
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert_eq!(
        runtime
            .block_on(fetch_installed(&base, "1.32.0"))
            .unwrap()
            .version,
        Version::new(1, 32, 0)
    );
    server.join().unwrap();
}
