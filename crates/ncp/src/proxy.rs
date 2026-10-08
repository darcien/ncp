//! The proxy route of an HTTP request, as reqwest takes it.
//!
//! reqwest reads the proxy variables, and then the manual HTTP and HTTPS
//! proxy of the macOS system settings (design C5). This module finds the
//! same route with the same matcher, so that `-v` and the errors can show
//! it, with its source. There is no exception for loopback hosts:
//! `NO_PROXY` is the only exception list.

use std::fmt;

use hyper_util::client::proxy::matcher::Matcher;

use crate::expand::Env;

const HTTP: [&str; 2] = ["HTTP_PROXY", "http_proxy"];
const HTTPS: [&str; 2] = ["HTTPS_PROXY", "https_proxy"];
const ALL: [&str; 2] = ["ALL_PROXY", "all_proxy"];
const NO: [&str; 2] = ["NO_PROXY", "no_proxy"];

/// Where the proxy setting comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Variable(&'static str),
    SystemSettings,
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Variable(name) => f.write_str(name),
            Self::SystemSettings => f.write_str("the system settings"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Direct,
    /// A proxy variable names a proxy, but `NO_PROXY` matches the host.
    DirectByNoProxy,
    /// The proxy URL has no user information: a password shows as `****`.
    Proxy {
        url: String,
        source: Source,
    },
}

impl Route {
    /// True when the proxy is on another host than the loopback host of the
    /// target. Such a proxy cannot reach a server on the loopback host of
    /// the user.
    #[must_use]
    pub fn sends_loopback_away(&self, target: &url::Url) -> bool {
        let Self::Proxy { url, .. } = self else {
            return false;
        };
        let loopback = |host: Option<url::Host<&str>>| match host {
            Some(url::Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            None => false,
        };
        let proxy_host = url::Url::parse(url).ok();
        loopback(target.host()) && !loopback(proxy_host.as_ref().and_then(url::Url::host))
    }
}

impl fmt::Display for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Direct => f.write_str("direct"),
            Self::DirectByNoProxy => f.write_str("direct, because NO_PROXY matches the host"),
            Self::Proxy { url, source } => write!(f, "proxy {url} from {source}"),
        }
    }
}

/// The first variable of the list that is set and not empty.
fn first<'a>(env: &'a Env, names: &[&'static str]) -> Option<(&'static str, &'a str)> {
    names.iter().find_map(|name| {
        env.get(*name)
            .filter(|value| !value.is_empty())
            .map(|value| (*name, value.as_str()))
    })
}

/// The route of a request to the URL.
#[must_use]
pub fn route(env: &Env, target: &url::Url) -> Route {
    route_with(env, target, Matcher::from_system)
}

/// The route, with the matcher of the system settings for the case with no
/// proxy variable. `Matcher::from_system` reads the process environment
/// too, which is the same environment in `ncp`.
fn route_with(env: &Env, target: &url::Url, system: impl FnOnce() -> Matcher) -> Route {
    let Ok(uri) = target.as_str().parse::<http::Uri>() else {
        return Route::Direct;
    };
    let https = target.scheme() == "https";
    // reqwest ignores HTTP_PROXY in a CGI program, because a CGI request
    // can set it from a header.
    let cgi = env.contains_key("REQUEST_METHOD");
    let for_scheme = if https {
        first(env, &HTTPS)
    } else if cgi {
        None
    } else {
        first(env, &HTTP)
    };
    let chosen = for_scheme.or_else(|| first(env, &ALL));
    let no_proxy = first(env, &NO).map(|(_, value)| value).unwrap_or_default();
    let (matcher, source) = match chosen {
        Some((name, value)) => {
            let builder = Matcher::builder().no(no_proxy);
            let builder = match (name, https) {
                (name, _) if ALL.contains(&name) => builder.all(value),
                (_, true) => builder.https(value),
                (_, false) => builder.http(value),
            };
            (builder.build(), Source::Variable(name))
        }
        None => (system(), Source::SystemSettings),
    };
    match matcher.intercept(&uri) {
        Some(intercept) => {
            let url = intercept.uri().to_string();
            let url = if intercept.basic_auth().is_some() {
                url.replacen("://", "://****@", 1)
            } else {
                url
            };
            Route::Proxy { url, source }
        }
        None if chosen.is_some() => Route::DirectByNoProxy,
        None => Route::Direct,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> Env {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn url(text: &str) -> url::Url {
        url::Url::parse(text).unwrap()
    }

    /// The route with no system proxy, so that the settings of the host do
    /// not change the result.
    fn route(env: &Env, target: &url::Url) -> Route {
        route_with(env, target, || Matcher::builder().build())
    }

    #[test]
    fn https_proxy_variable_names_the_route_and_hides_the_password() {
        let route = route(
            &env(&[("HTTPS_PROXY", "http://user:pa%40ss@proxy.test:3128")]),
            &url("https://api.example.com/mcp"),
        );
        assert_eq!(
            route,
            Route::Proxy {
                url: "http://****@proxy.test:3128/".into(),
                source: Source::Variable("HTTPS_PROXY")
            }
        );
        assert!(!route.to_string().contains("pa"));
    }

    #[test]
    fn upper_case_comes_first_and_https_proxy_is_only_for_https() {
        let both = env(&[
            ("HTTPS_PROXY", "http://upper.test:1"),
            ("https_proxy", "http://lower.test:1"),
        ]);
        assert!(matches!(
            route(&both, &url("https://a.example/")),
            Route::Proxy {
                source: Source::Variable("HTTPS_PROXY"),
                ..
            }
        ));
        assert_eq!(
            route(
                &env(&[("HTTPS_PROXY", "http://p.test:1")]),
                &url("http://localhost/x")
            ),
            Route::Direct
        );
    }

    #[test]
    fn all_proxy_is_the_fallback_and_cgi_ignores_http_proxy() {
        assert!(matches!(
            route(
                &env(&[("ALL_PROXY", "http://all.test:1")]),
                &url("https://a.example/")
            ),
            Route::Proxy {
                source: Source::Variable("ALL_PROXY"),
                ..
            }
        ));
        assert_eq!(
            route(
                &env(&[("HTTP_PROXY", "http://p.test:1"), ("REQUEST_METHOD", "GET")]),
                &url("http://localhost/x")
            ),
            Route::Direct
        );
    }

    #[test]
    fn no_proxy_makes_the_route_direct() {
        let vars = env(&[
            ("HTTP_PROXY", "http://p.test:1"),
            ("NO_PROXY", "localhost,127.0.0.1,::1"),
        ]);
        assert_eq!(
            route(&vars, &url("http://127.0.0.1:8080/mcp")),
            Route::DirectByNoProxy
        );
    }

    #[test]
    fn a_proxy_on_another_host_sends_loopback_requests_away() {
        let target = url("http://127.0.0.1:8080/mcp");
        let vars = env(&[("HTTP_PROXY", "http://proxy.test:3128")]);
        assert!(route(&vars, &target).sends_loopback_away(&target));
        let local = env(&[("HTTP_PROXY", "http://127.0.0.1:3128")]);
        assert!(!route(&local, &target).sends_loopback_away(&target));
        assert!(!Route::Direct.sends_loopback_away(&target));
    }
}
