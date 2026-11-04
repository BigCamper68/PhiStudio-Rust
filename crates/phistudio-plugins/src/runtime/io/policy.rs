use phistudio_sdk::NetworkPermissions;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

pub(super) fn url_allowed(
    url: &url::Url,
    permissions: &NetworkPermissions,
) -> Result<(), &'static str> {
    if !matches!(url.scheme(), "http" | "https" | "ws" | "wss")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || !permissions
            .origins
            .contains(&url.origin().ascii_serialization())
        || (matches!(url.scheme(), "http" | "ws") && !permissions.local_network)
    {
        return Err("origin_denied");
    }
    Ok(())
}

fn v4_kind(ip: Ipv4Addr) -> u8 {
    let [a, b, c, _] = ip.octets();
    if a == 127 || a == 10 || (a == 172 && (16..=31).contains(&b)) || (a == 192 && b == 168) {
        return 1;
    }
    if a == 0
        || a >= 224
        || (a == 169 && b == 254)
        || (a == 100 && (64..=127).contains(&b))
        || (a == 192 && b == 0 && (c == 0 || c == 2))
        || (a == 192 && b == 88 && c == 99)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
    {
        return 0;
    }
    2
}

pub(super) fn address_allowed(
    ip: IpAddr,
    permissions: &NetworkPermissions,
    cleartext: bool,
) -> bool {
    let kind = match ip {
        IpAddr::V4(ip) => v4_kind(ip),
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                v4_kind(v4)
            } else if ip.is_loopback() || (ip.segments()[0] & 0xfe00) == 0xfc00 {
                1
            } else if (ip.segments()[0] & 0xe000) == 0x2000
                && ip.segments()[0] != 0x2002
                && !(ip.segments()[0] == 0x2001 && ip.segments()[1] == 0x0db8)
                && !(ip.segments()[0] == 0x2001 && ip.segments()[1] < 0x0200)
            {
                2
            } else {
                0
            }
        }
    };
    match kind {
        1 => permissions.local_network,
        2 => !cleartext,
        _ => false,
    }
}

pub(super) async fn resolve(
    url: &url::Url,
    permissions: &NetworkPermissions,
) -> Result<Vec<SocketAddr>, &'static str> {
    url_allowed(url, permissions)?;
    let host = url.host_str().ok_or("invalid_url")?;
    let port = url.port_or_known_default().ok_or("invalid_url")?;
    let addresses: Vec<_> = match url.host().ok_or("invalid_url")? {
        url::Host::Ipv4(ip) => vec![SocketAddr::new(ip.into(), port)],
        url::Host::Ipv6(ip) => vec![SocketAddr::new(ip.into(), port)],
        url::Host::Domain(_) => tokio::net::lookup_host((host, port))
            .await
            .map_err(|_| "dns_failed")?
            .take(17)
            .collect(),
    };
    if addresses.is_empty()
        || addresses.len() > 16
        || addresses
            .iter()
            .any(|a| !address_allowed(a.ip(), permissions, matches!(url.scheme(), "http" | "ws")))
    {
        return Err("address_denied");
    }
    Ok(addresses)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_private_and_transition_addresses_have_separate_boundaries() {
        let public = NetworkPermissions::default();
        let local = NetworkPermissions {
            local_network: true,
            ..Default::default()
        };
        for address in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "::1",
            "fd12::1",
            "::ffff:127.0.0.1",
        ] {
            let ip: IpAddr = address.parse().unwrap();
            assert!(!address_allowed(ip, &public, false), "{address}");
            assert!(address_allowed(ip, &local, true), "{address}");
        }
        for address in [
            "0.0.0.0",
            "169.254.169.254",
            "100.64.0.1",
            "198.18.0.1",
            "192.0.2.1",
            "224.0.0.1",
            "255.255.255.255",
            "::",
            "fe80::1",
            "ff02::1",
            "64:ff9b::7f00:1",
            "2002:7f00:1::",
            "2001::1",
            "2001:db8::1",
        ] {
            let ip: IpAddr = address.parse().unwrap();
            assert!(!address_allowed(ip, &local, false), "{address}");
        }
        for address in [
            "8.8.8.8",
            "1.1.1.1",
            "2606:4700:4700::1111",
            "::ffff:8.8.8.8",
        ] {
            let ip: IpAddr = address.parse().unwrap();
            assert!(address_allowed(ip, &public, false), "{address}");
            assert!(!address_allowed(ip, &local, true), "{address}");
        }
    }
}
