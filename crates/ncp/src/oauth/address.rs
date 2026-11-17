//! The address policy of the OAuth requests.
//!
//! A server names the URLs of its OAuth requests, so a hostile server can
//! send them to a service on the network or on the computer of the user
//! (MCP 2026-07-28, "Security Best Practices", "Server-Side Request
//! Forgery (SSRF)").
//!
//! Permission comes only from the entry. Server responses and DNS answers
//! never add to it, because the server can control both.
//!
//! The address classes copy those of the OAuth client of rmcp 3.10.0, so
//! that the two checks agree about each address. When the version of rmcp
//! changes, compare the ranges.

use std::{
    fmt,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
};

use url::{Host, Url};

use crate::proxy::{Route, Source};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressClass {
    /// Not a promise that the internet can reach the address.
    Public,
    Private,
    Loopback,
    /// Includes link-local addresses, which hold the metadata service of a
    /// cloud computer.
    Blocked,
}

impl AddressClass {
    /// An IPv6 address with an embedded IPv4 address gets the class of the
    /// IPv4 address, so that a translation prefix cannot hide an internal
    /// address.
    #[must_use]
    pub fn of(address: IpAddr) -> Self {
        match address {
            IpAddr::V4(v4) => Self::of_ipv4(v4),
            IpAddr::V6(v6) => embedded_ipv4(v6).map_or_else(|| Self::of_ipv6(v6), Self::of_ipv4),
        }
    }

    fn of_ipv4(address: Ipv4Addr) -> Self {
        let [first, second, ..] = address.octets();
        let shared = first == 100 && (64..=127).contains(&second);
        if address.is_loopback() {
            Self::Loopback
        } else if address.is_private() || shared {
            Self::Private
        } else if address.is_link_local()
            || address.is_broadcast()
            || address.is_unspecified()
            || address.is_multicast()
            || first == 0
            || is_benchmark_ipv4(address)
        {
            Self::Blocked
        } else {
            Self::Public
        }
    }

    fn of_ipv6(address: Ipv6Addr) -> Self {
        let segments = address.segments();
        let unique_local = segments[0] & 0xfe00 == 0xfc00;
        let link_local = segments[0] & 0xffc0 == 0xfe80;
        let teredo = segments[..2] == [0x2001, 0];
        let local_nat64 = segments[..3] == [0x64, 0xff9b, 1];
        if address.is_loopback() {
            Self::Loopback
        } else if unique_local {
            Self::Private
        } else if address.is_unspecified()
            || address.is_multicast()
            || link_local
            || teredo
            || local_nat64
        {
            Self::Blocked
        } else {
            Self::Public
        }
    }
}

/// The order goes from the class that the user can most probably permit
/// to the class that no permission gives. When DNS gives only refused
/// addresses, the stop names the first of them in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    Private,
    Loopback,
    Blocked,
}

impl Refused {
    fn phrase(self) -> &'static str {
        match self {
            Self::Private => "a private address",
            Self::Loopback => "a loopback address",
            Self::Blocked => "an address in a blocked range",
        }
    }

    fn rule(self) -> &'static str {
        match self {
            Self::Private => "The entry does not permit private addresses.",
            Self::Loopback => "Only an entry with a loopback URL permits loopback addresses.",
            Self::Blocked => "ncp sends no OAuth request to this range.",
        }
    }
}

/// Teredo and local-use NAT64 addresses are blocked as a whole, so they
/// need no case here.
fn embedded_ipv4(address: Ipv6Addr) -> Option<Ipv4Addr> {
    let ipv4 = |high: u16, low: u16| Ipv4Addr::from((u32::from(high) << 16) | u32::from(low));
    match address.segments() {
        [0, 0, 0, 0, 0, 0xffff, high, low] | [0x64, 0xff9b, 0, 0, 0, 0, high, low] => {
            Some(ipv4(high, low))
        }
        [0, 0, 0, 0, 0, 0, high, low] if !address.is_loopback() && !address.is_unspecified() => {
            Some(ipv4(high, low))
        }
        [0x2002, high, low, ..] => Some(ipv4(high, low)),
        _ => None,
    }
}

#[must_use]
pub fn is_benchmark(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => is_benchmark_ipv4(v4),
        IpAddr::V6(v6) => embedded_ipv4(v6).is_some_and(is_benchmark_ipv4),
    }
}

fn is_benchmark_ipv4(address: Ipv4Addr) -> bool {
    matches!(address.octets(), [198, 18 | 19, ..])
}

// These names are for the computer of the user (RFC 6761 §6.3).
fn is_localhost_name(name: &str) -> bool {
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    name == "localhost" || name.ends_with(".localhost")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Found {
    Literal(IpAddr),
    LocalhostName,
    Resolved(IpAddr),
}

fn found_in_text(host: &Host<&str>) -> Option<(Found, AddressClass)> {
    match host {
        Host::Ipv4(v4) => {
            let address = IpAddr::V4(*v4);
            Some((Found::Literal(address), AddressClass::of(address)))
        }
        Host::Ipv6(v6) => {
            let address = IpAddr::V6(*v6);
            Some((Found::Literal(address), AddressClass::of(address)))
        }
        Host::Domain(name) if is_localhost_name(name) => {
            Some((Found::LocalhostName, AddressClass::Loopback))
        }
        Host::Domain(_) => None,
    }
}

// An IPv4 loopback address inside an IPv6 address does not count, because
// a plain-text request must not depend on a translation of the address.
fn is_loopback_host(host: &Host<&str>) -> bool {
    match host {
        Host::Domain(name) => is_localhost_name(name),
        Host::Ipv4(v4) => v4.is_loopback(),
        Host::Ipv6(v6) => v6.is_loopback(),
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Permission {
    loopback: bool,
    private: bool,
}

impl Permission {
    /// A host name permits no class, whatever its DNS answer is, because
    /// the owner of the name controls that answer.
    #[must_use]
    pub fn of_entry(url: &Url, allow_private_addresses: bool) -> Self {
        let class = url
            .host()
            .as_ref()
            .and_then(found_in_text)
            .map(|(_, class)| class);
        Self {
            loopback: class == Some(AddressClass::Loopback),
            private: class == Some(AddressClass::Private) || allow_private_addresses,
        }
    }

    #[must_use]
    pub fn refuses(self, class: AddressClass) -> Option<Refused> {
        match class {
            AddressClass::Public => None,
            AddressClass::Private => (!self.private).then_some(Refused::Private),
            AddressClass::Loopback => (!self.loopback).then_some(Refused::Loopback),
            AddressClass::Blocked => Some(Refused::Blocked),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    NotValid,
    Scheme,
    Class {
        found: Found,
        class: Refused,
    },
    NotLoopback {
        address: IpAddr,
    },
    /// `proxy` has its password hidden.
    RemoteProxy {
        proxy: String,
        source: Source,
    },
    /// The client follows no redirect, because the target got no check.
    /// `location` has its secrets hidden.
    Redirect {
        status: u16,
        location: String,
    },
}

impl StopReason {
    #[must_use]
    pub fn describe(
        &self,
        host: &str,
        on_entry_host: bool,
        quote: &dyn Fn(&str) -> String,
    ) -> String {
        match self {
            Self::NotValid => "The URL cannot be parsed".to_string(),
            Self::Scheme => format!(
                "The URL does not use {}, and its host is not a loopback host",
                quote("https")
            ),
            Self::Class {
                found: Found::Resolved(address),
                class: Refused::Loopback,
            } if on_entry_host => format!(
                "The host {} of the entry resolves to {}, which is a loopback address. \
                 No key of the entry permits a loopback address for a host name.",
                quote(host),
                quote(&address.to_string())
            ),
            Self::Class { found, class } => {
                let fact = match found {
                    Found::Literal(_) => format!("The host {} is {}.", quote(host), class.phrase()),
                    Found::LocalhostName => {
                        format!("The host {} is a localhost name.", quote(host))
                    }
                    Found::Resolved(address) => format!(
                        "The host {} resolves to {}, which is {}.",
                        quote(host),
                        quote(&address.to_string()),
                        class.phrase()
                    ),
                };
                format!("{fact} {}", class.rule())
            }
            Self::NotLoopback { address } => format!(
                "The host {} is a localhost name. It resolves to {}, which is not a loopback address.",
                quote(host),
                quote(&address.to_string())
            ),
            Self::RemoteProxy { proxy, source } => {
                let source = match source {
                    Source::Variable(name) => quote(name),
                    Source::SystemSettings => source.to_string(),
                };
                format!(
                    "The URL uses {}, and the proxy {} from {source} is on another computer",
                    quote("http"),
                    quote(proxy)
                )
            }
            Self::Redirect { status, location } => format!(
                "The authorization server answered {status} with a redirect to {}. ncp does not follow this redirect.",
                quote(location)
            ),
        }
    }
}

/// `url` has its secrets hidden. For a redirect, `url` is the request that
/// got the redirect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafetyStop {
    pub url: String,
    pub host: String,
    pub on_entry_host: bool,
    pub reason: StopReason,
}

impl SafetyStop {
    #[must_use]
    pub fn describe(&self, quote: &dyn Fn(&str) -> String) -> String {
        self.reason.describe(&self.host, self.on_entry_host, quote)
    }
}

/// rmcp keeps this text as its error. `ncp` builds the full message from
/// the connect notes, so this text only names the kind of error.
impl fmt::Display for SafetyStop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ncp stopped the OAuth request on purpose")
    }
}

impl std::error::Error for SafetyStop {}

/// Checks the text of a URL before a request. A host name passes here,
/// because [`permitted_addresses`] checks its DNS answer at connect time.
///
/// # Errors
///
/// Returns the reason when the request must not go out.
pub fn check_url(url: &Url, permission: Permission, route: &Route) -> Result<(), StopReason> {
    let host = url.host().ok_or(StopReason::Scheme)?;
    let plain = match url.scheme() {
        "https" => false,
        "http" if is_loopback_host(&host) => true,
        _ => return Err(StopReason::Scheme),
    };
    if let Some((found, class)) =
        found_in_text(&host).and_then(|(found, class)| Some((found, permission.refuses(class)?)))
    {
        return Err(StopReason::Class { found, class });
    }
    match route {
        Route::Proxy { url, source } if plain && !is_local_proxy(url) => {
            Err(StopReason::RemoteProxy {
                proxy: url.clone(),
                source: source.clone(),
            })
        }
        _ => Ok(()),
    }
}

// A proxy on the computer of the user keeps a plain-text request on that
// computer.
fn is_local_proxy(proxy: &str) -> bool {
    Url::parse(proxy)
        .ok()
        .as_ref()
        .and_then(Url::host)
        .is_some_and(|host| is_loopback_host(&host))
}

/// The addresses of a DNS answer that the entry permits for `host`.
///
/// A localhost name keeps only loopback addresses, because the name stands
/// for the computer of the user. An empty answer stays empty, so the
/// connection fails as for any name with no address.
///
/// # Errors
///
/// Returns the stop for the refused address that the user can most
/// probably fix (see [`Refused`]) when no address is permitted.
pub fn permitted_addresses(
    host: &str,
    answer: Vec<SocketAddr>,
    permission: Permission,
) -> Result<Vec<SocketAddr>, StopReason> {
    let localhost = is_localhost_name(host);
    let verdict = |socket: SocketAddr| {
        let address = socket.ip();
        let class = AddressClass::of(address);
        match permission.refuses(class) {
            _ if localhost && class != AddressClass::Loopback => {
                Err(StopReason::NotLoopback { address })
            }
            Some(refused) => Err(StopReason::Class {
                found: Found::Resolved(address),
                class: refused,
            }),
            None => Ok(socket),
        }
    };
    let verdicts: Vec<Result<SocketAddr, StopReason>> = answer.into_iter().map(verdict).collect();
    let permitted: Vec<SocketAddr> = verdicts
        .iter()
        .filter_map(|v| v.as_ref().ok())
        .copied()
        .collect();
    if !permitted.is_empty() {
        return Ok(permitted);
    }
    verdicts
        .into_iter()
        .filter_map(Result::err)
        .min_by_key(priority)
        .map_or(Ok(Vec::new()), Err)
}

// A localhost name with no loopback address comes last, because no
// permission fixes it.
fn priority(reason: &StopReason) -> u8 {
    match reason {
        StopReason::Class {
            class: Refused::Private,
            ..
        } => 0,
        StopReason::Class {
            class: Refused::Loopback,
            ..
        } => 1,
        StopReason::Class {
            class: Refused::Blocked,
            ..
        } => 2,
        _ => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(text: &str) -> AddressClass {
        AddressClass::of(text.parse().unwrap())
    }

    #[test]
    fn classifies_addresses_as_the_sdk_does() {
        for (text, expected) in [
            ("93.184.216.34", AddressClass::Public),
            ("2606:4700::1111", AddressClass::Public),
            ("127.0.0.1", AddressClass::Loopback),
            ("127.255.255.255", AddressClass::Loopback),
            ("::1", AddressClass::Loopback),
            ("10.0.0.9", AddressClass::Private),
            ("172.16.0.1", AddressClass::Private),
            ("172.31.255.255", AddressClass::Private),
            ("192.168.1.1", AddressClass::Private),
            ("100.64.0.1", AddressClass::Private),
            ("100.127.255.255", AddressClass::Private),
            ("fc00::1", AddressClass::Private),
            ("fdff::1", AddressClass::Private),
            ("169.254.169.254", AddressClass::Blocked),
            ("0.0.0.0", AddressClass::Blocked),
            ("0.1.2.3", AddressClass::Blocked),
            ("198.18.0.1", AddressClass::Blocked),
            ("198.19.255.255", AddressClass::Blocked),
            ("224.0.0.1", AddressClass::Blocked),
            ("239.255.255.255", AddressClass::Blocked),
            ("255.255.255.255", AddressClass::Blocked),
            ("::", AddressClass::Blocked),
            ("ff02::1", AddressClass::Blocked),
            ("fe80::1", AddressClass::Blocked),
            ("febf::1", AddressClass::Blocked),
            ("2001:0:4136:e378::1", AddressClass::Blocked),
            ("64:ff9b:1::a9fe:a9fe", AddressClass::Blocked),
        ] {
            assert_eq!(class(text), expected, "{text}");
        }
    }

    #[test]
    fn range_boundaries_fall_on_the_public_side() {
        for text in [
            "9.255.255.255",
            "11.0.0.0",
            "172.15.255.255",
            "172.32.0.0",
            "100.63.255.255",
            "100.128.0.0",
            "169.253.255.255",
            "198.17.255.255",
            "198.20.0.0",
            "223.255.255.255",
            "fbff::1",
            "fe00::1",
            "fec0::1",
            "feff::1",
            "1.0.0.0",
            "126.255.255.255",
            "128.0.0.0",
            "170.0.0.0",
            "192.167.255.255",
            "192.169.0.0",
            "2001:1::1",
            "64:ff9b:2::1",
        ] {
            assert_eq!(class(text), AddressClass::Public, "{text}");
        }
    }

    #[test]
    fn special_purpose_addresses_stay_public_as_in_the_sdk() {
        assert_eq!(class("240.0.0.1"), AddressClass::Public);
        assert_eq!(class("2001:db8::1"), AddressClass::Public);
    }

    #[test]
    fn embedded_ipv4_addresses_get_the_class_of_the_inner_address() {
        for (prefix, inner) in [
            ("::ffff:", "mapped"),
            ("::", "compatible"),
            ("64:ff9b::", "nat64"),
        ] {
            for (address, expected) in [
                ("93.184.216.34", AddressClass::Public),
                ("10.0.0.9", AddressClass::Private),
                ("127.0.0.1", AddressClass::Loopback),
                ("169.254.169.254", AddressClass::Blocked),
            ] {
                let text = format!("{prefix}{address}");
                assert_eq!(class(&text), expected, "{inner} {text}");
            }
        }
        assert_eq!(class("2002:5db8:d822::"), AddressClass::Public);
        assert_eq!(class("2002:a00:9::"), AddressClass::Private);
        assert_eq!(class("2002:7f00:1::"), AddressClass::Loopback);
        assert_eq!(class("2002:a9fe:a9fe::"), AddressClass::Blocked);
    }

    #[test]
    fn localhost_names_end_in_localhost_with_or_without_a_final_dot() {
        for name in [
            "localhost",
            "LOCALHOST",
            "localhost.",
            "a.localhost",
            "a.b.localhost.",
        ] {
            assert!(is_localhost_name(name), "{name}");
        }
        for name in ["localhost.example", "mylocalhost", "example.com", ""] {
            assert!(!is_localhost_name(name), "{name}");
        }
    }

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    fn entry(text: &str, private: bool) -> Permission {
        Permission::of_entry(&url(text), private)
    }

    #[test]
    fn the_entry_url_permits_its_own_class_only() {
        use AddressClass::{Loopback, Private, Public};
        let classes = [Public, Private, Loopback, AddressClass::Blocked];
        let permitted = |permission: Permission| -> Vec<AddressClass> {
            classes
                .into_iter()
                .filter(|class| permission.refuses(*class).is_none())
                .collect()
        };
        for (text, private, expected) in [
            ("https://mcp.example.com/mcp", false, vec![Public]),
            ("https://mcp.example.com/mcp", true, vec![Public, Private]),
            ("http://127.0.0.1:8080/mcp", false, vec![Public, Loopback]),
            ("http://localhost/mcp", false, vec![Public, Loopback]),
            ("https://a.localhost./mcp", false, vec![Public, Loopback]),
            ("http://[::1]/mcp", true, vec![Public, Private, Loopback]),
            ("https://10.0.0.5/mcp", false, vec![Public, Private]),
            (
                "https://[::ffff:10.0.0.5]/mcp",
                false,
                vec![Public, Private],
            ),
            ("https://169.254.169.254/mcp", true, vec![Public, Private]),
        ] {
            assert_eq!(
                permitted(entry(text, private)),
                expected,
                "{text} {private}"
            );
        }
    }

    #[test]
    fn parser_forms_of_one_address_give_one_decision() {
        let public = entry("https://mcp.example.com/mcp", false);
        for text in [
            "https://127.1/x",
            "https://0x7f.0.0.1/x",
            "https://2130706433/x",
            "https://[::ffff:7f00:1]/x",
        ] {
            assert!(
                matches!(
                    check_url(&url(text), public, &Route::Direct),
                    Err(StopReason::Class {
                        found: Found::Literal(_),
                        class: Refused::Loopback
                    })
                ),
                "{text}"
            );
        }
    }

    #[test]
    fn the_scheme_must_be_https_or_http_on_a_loopback_host() {
        let local = entry("http://127.0.0.1/mcp", true);
        for text in [
            "http://127.0.0.1:9/x",
            "http://[::1]/x",
            "http://localhost/x",
            "http://a.localhost./x",
            "https://as.example/x",
        ] {
            assert_eq!(
                check_url(&url(text), local, &Route::Direct),
                Ok(()),
                "{text}"
            );
        }
        for text in [
            "http://as.example/x",
            "http://10.0.0.5/x",
            "http://[::ffff:127.0.0.1]/x",
            "ftp://as.example/x",
            "file:///etc/passwd",
        ] {
            assert_eq!(
                check_url(&url(text), local, &Route::Direct),
                Err(StopReason::Scheme),
                "{text}"
            );
        }
    }

    #[test]
    fn a_public_entry_cannot_reach_loopback_also_with_the_private_option() {
        for private in [false, true] {
            let permission = entry("https://mcp.example.com/mcp", private);
            for text in ["http://127.0.0.1:9/register", "https://localhost/token"] {
                assert!(
                    matches!(
                        check_url(&url(text), permission, &Route::Direct),
                        Err(StopReason::Class {
                            class: Refused::Loopback,
                            ..
                        })
                    ),
                    "{text} {private}"
                );
            }
        }
    }

    #[test]
    fn a_blocked_literal_stays_blocked_also_as_the_host_of_the_entry() {
        let permission = entry("https://169.254.169.254/mcp", true);
        assert_eq!(
            check_url(
                &url("https://169.254.169.254/.well-known/oauth-protected-resource"),
                permission,
                &Route::Direct
            ),
            Err(StopReason::Class {
                found: Found::Literal("169.254.169.254".parse().unwrap()),
                class: Refused::Blocked
            })
        );
    }

    fn proxy(url: &str) -> Route {
        Route::Proxy {
            url: url.into(),
            source: Source::Variable("HTTP_PROXY"),
        }
    }

    #[test]
    fn plain_http_goes_to_no_proxy_on_another_computer() {
        let permission = entry("http://127.0.0.1/mcp", false);
        let target = url("http://127.0.0.1:9/token");
        assert_eq!(
            check_url(&target, permission, &proxy("http://proxy.test:3128/")),
            Err(StopReason::RemoteProxy {
                proxy: "http://proxy.test:3128/".into(),
                source: Source::Variable("HTTP_PROXY")
            })
        );
        assert_eq!(
            check_url(&target, permission, &proxy("http://127.0.0.1:3128/")),
            Ok(())
        );
        assert_eq!(
            check_url(
                &url("https://as.example/token"),
                permission,
                &proxy("http://proxy.test:3128/")
            ),
            Ok(())
        );
    }

    fn addresses(texts: &[&str]) -> Vec<SocketAddr> {
        texts
            .iter()
            .map(|text| SocketAddr::new(text.parse().unwrap(), 443))
            .collect()
    }

    #[test]
    fn dns_answers_keep_only_permitted_addresses() {
        let public = entry("https://mcp.example.com/mcp", false);
        assert_eq!(
            permitted_addresses(
                "as.example",
                addresses(&["10.0.0.5", "93.184.216.34"]),
                public
            ),
            Ok(addresses(&["93.184.216.34"]))
        );
        assert_eq!(
            permitted_addresses("as.example", Vec::new(), public),
            Ok(Vec::new())
        );
    }

    #[test]
    fn a_private_answer_stops_by_default_and_passes_with_the_option() {
        let answer = addresses(&["10.0.0.5"]);
        assert_eq!(
            permitted_addresses(
                "as.corp.example",
                answer.clone(),
                entry("https://mcp.corp.example/mcp", false)
            ),
            Err(StopReason::Class {
                found: Found::Resolved("10.0.0.5".parse().unwrap()),
                class: Refused::Private
            })
        );
        assert_eq!(
            permitted_addresses(
                "as.corp.example",
                answer.clone(),
                entry("https://mcp.corp.example/mcp", true)
            ),
            Ok(answer)
        );
    }

    #[test]
    fn dns_answers_never_add_permission() {
        let permission = entry("https://mcp.example.com/mcp", false);
        for internal in ["127.0.0.1", "10.0.0.5", "169.254.169.254"] {
            assert!(
                permitted_addresses("mcp.example.com", addresses(&[internal]), permission).is_err(),
                "{internal}"
            );
        }
    }

    #[test]
    fn the_stop_names_the_refused_address_that_the_user_can_most_probably_permit() {
        let permission = entry("https://mcp.example.com/mcp", false);
        assert_eq!(
            permitted_addresses(
                "as.example",
                addresses(&["169.254.169.254", "127.0.0.1", "10.0.0.5"]),
                permission
            ),
            Err(StopReason::Class {
                found: Found::Resolved("10.0.0.5".parse().unwrap()),
                class: Refused::Private
            })
        );
    }

    #[test]
    fn a_localhost_name_keeps_only_loopback_addresses() {
        let permission = entry("http://localhost:8080/mcp", true);
        assert_eq!(
            permitted_addresses(
                "auth.localhost",
                addresses(&["93.184.216.34", "10.0.0.5", "127.0.0.1"]),
                permission
            ),
            Ok(addresses(&["127.0.0.1"]))
        );
        assert_eq!(
            permitted_addresses("auth.localhost", addresses(&["93.184.216.34"]), permission),
            Err(StopReason::NotLoopback {
                address: "93.184.216.34".parse().unwrap()
            })
        );
    }

    #[test]
    fn reasons_follow_the_message_rules() {
        let code = |text: &str| crate::text::code(text);
        let address = |text: &str| text.parse().unwrap();
        let texts = [
            StopReason::NotValid.describe("", false, &code),
            StopReason::Scheme.describe("as.example", false, &code),
            StopReason::Class {
                found: Found::Resolved(address("10.0.0.5")),
                class: Refused::Private,
            }
            .describe("as.corp", false, &code),
            StopReason::Class {
                found: Found::Literal(address("10.0.0.5")),
                class: Refused::Private,
            }
            .describe("10.0.0.5", false, &code),
            StopReason::Class {
                found: Found::LocalhostName,
                class: Refused::Loopback,
            }
            .describe("localhost", false, &code),
            StopReason::Class {
                found: Found::Literal(address("fe80::1")),
                class: Refused::Blocked,
            }
            .describe("[fe80::1]", false, &code),
            StopReason::NotLoopback {
                address: "93.184.216.34".parse().unwrap(),
            }
            .describe("a.localhost", false, &code),
            StopReason::RemoteProxy {
                proxy: "http://proxy.test:3128/".into(),
                source: Source::SystemSettings,
            }
            .describe("127.0.0.1", false, &code),
            StopReason::Redirect {
                status: 307,
                location: "https://other.example/register".into(),
            }
            .describe("as.example", false, &code),
            StopReason::Class {
                found: Found::Resolved(address("127.0.0.1")),
                class: Refused::Loopback,
            }
            .describe("app.test", true, &code),
        ];
        crate::style::assert_style(&texts);
        assert_eq!(
            texts[2],
            "The host `as.corp` resolves to `10.0.0.5`, which is a private address. \
             The entry does not permit private addresses."
        );
        assert_eq!(
            texts[3],
            "The host `10.0.0.5` is a private address. The entry does not permit private addresses."
        );
        assert_eq!(
            texts[4],
            "The host `localhost` is a localhost name. \
             Only an entry with a loopback URL permits loopback addresses."
        );
        assert_eq!(
            texts[9],
            "The host `app.test` of the entry resolves to `127.0.0.1`, which is a loopback \
             address. No key of the entry permits a loopback address for a host name."
        );
    }
}
