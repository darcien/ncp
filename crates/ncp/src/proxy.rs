//! The proxy route of an HTTP request, as reqwest takes it.
//!
//! reqwest reads the proxy variables and the manual HTTP and HTTPS proxy
//! of the macOS system settings, in the order of uv (design C5). This
//! module finds the same route with the same matcher, so that `-v` and the
//! errors can show it, with its source. There is no exception for loopback
//! hosts: `NO_PROXY` is the only exception list.

use std::fmt;

use hyper_util::client::proxy::matcher::{Intercept, Matcher};

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
    /// A proxy variable names a proxy, but `NO_PROXY` matches the host. When
    /// only the system settings name a proxy, the route shows as `Direct`:
    /// the matcher does not tell which proxy `NO_PROXY` stopped.
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
        let loopback = |host: Option<url::Host<&str>>| {
            host.is_some_and(|host| crate::server::is_loopback(&host))
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

/// The first variable of the list that is set, also when it is empty, as
/// reqwest reads them. A value that reqwest does not take as a proxy, for
/// example `ftp://p:1`, names no proxy.
fn first_set<'a>(env: &'a Env, names: &[&'static str]) -> Option<(&'static str, &'a str)> {
    names
        .iter()
        .find_map(|name| env.get(*name).map(|value| (*name, value.as_str())))
        .filter(|(_, value)| names_a_proxy(value))
}

/// True when the matcher of reqwest takes the value as a proxy.
fn names_a_proxy(value: &str) -> bool {
    let probe = http::Uri::from_static("http://probe.invalid/");
    Matcher::builder()
        .all(value.to_string())
        .build()
        .intercept(&probe)
        .is_some()
}

fn no_proxy_is_set(env: &Env) -> bool {
    NO.iter()
        .find_map(|name| env.get(*name))
        .is_some_and(|value| !value.is_empty())
}

/// The route of a request to the URL.
#[must_use]
pub fn route(env: &Env, target: &url::Url) -> Route {
    route_with(env, target, Matcher::from_system, Matcher::from_env)
}

/// The route from the matcher of reqwest. The source is a variable when the
/// matcher of the variables alone gives the same proxy. Else the proxy comes
/// from the system settings: reqwest reads them for a scheme with no
/// variable of its own, before `ALL_PROXY`. When the system settings give
/// the same proxy as the variable, the variable shows as the source. Both
/// matchers read the process environment, which is the same environment in
/// `ncp`.
fn route_with(
    env: &Env,
    target: &url::Url,
    system: impl FnOnce() -> Matcher,
    variables: impl FnOnce() -> Matcher,
) -> Route {
    let Ok(uri) = target.as_str().parse::<http::Uri>() else {
        return Route::Direct;
    };
    // reqwest uses no proxy in a CGI program, because a CGI request can
    // set a proxy variable from a header.
    if env.contains_key("REQUEST_METHOD") {
        return Route::Direct;
    }
    let scheme = if target.scheme() == "https" {
        &HTTPS
    } else {
        &HTTP
    };
    let variable = first_set(env, scheme).or_else(|| first_set(env, &ALL));
    match system().intercept(&uri) {
        Some(intercept) => {
            let url = intercept.uri().to_string();
            let url = if intercept.basic_auth().is_some() {
                url.replacen("://", "://****@", 1)
            } else {
                url
            };
            let same = |other: &Intercept| {
                other.uri() == intercept.uri() && other.basic_auth() == intercept.basic_auth()
            };
            let from_variables = variables()
                .intercept(&uri)
                .is_some_and(|other| same(&other));
            let source = match variable {
                Some((name, _)) if from_variables => Source::Variable(name),
                _ => Source::SystemSettings,
            };
            Route::Proxy { url, source }
        }
        None if variable.is_some() && no_proxy_is_set(env) => Route::DirectByNoProxy,
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

    /// The value of the first variable that is set, as hyper-util reads it.
    fn value(env: &Env, names: &[&str]) -> String {
        names
            .iter()
            .find_map(|name| env.get(*name).cloned())
            .unwrap_or_default()
    }

    /// The matcher of reqwest with an HTTPS proxy in the system settings, or
    /// none. It copies `Matcher::from_system` of hyper-util: the settings
    /// give the proxy of a scheme with no value in its variable, before
    /// `ALL_PROXY`. The tests thus check the label, not the order of reqwest.
    fn matcher(env: &Env, system_https: &str) -> Matcher {
        let https = Some(value(env, &HTTPS))
            .filter(|https| !https.is_empty())
            .unwrap_or_else(|| system_https.to_string());
        Matcher::builder()
            .http(value(env, &HTTP))
            .https(https)
            .all(value(env, &ALL))
            .no(value(env, &NO))
            .build()
    }

    /// The route with no proxy in the system settings, so that the
    /// settings of the host do not change the result.
    fn route(env: &Env, target: &url::Url) -> Route {
        route_with_system(env, target, "")
    }

    fn route_with_system(env: &Env, target: &url::Url, system_https: &str) -> Route {
        route_with(
            env,
            target,
            || matcher(env, system_https),
            || matcher(env, ""),
        )
    }

    #[test]
    fn the_system_settings_name_the_route_with_no_variable() {
        let route = route_with_system(
            &env(&[]),
            &url("https://a.example/"),
            "http://sys.test:8080",
        );
        assert_eq!(
            route,
            Route::Proxy {
                url: "http://sys.test:8080/".into(),
                source: Source::SystemSettings
            }
        );
        assert_eq!(
            route.to_string(),
            "proxy http://sys.test:8080/ from the system settings"
        );
    }

    #[test]
    fn the_variable_of_the_scheme_comes_before_the_system_settings() {
        let vars = env(&[("HTTPS_PROXY", "http://var.test:1")]);
        let route = route_with_system(&vars, &url("https://a.example/"), "http://sys.test:8080");
        assert_eq!(
            route,
            Route::Proxy {
                url: "http://var.test:1/".into(),
                source: Source::Variable("HTTPS_PROXY")
            }
        );
    }

    #[test]
    fn a_variable_that_reqwest_does_not_take_names_no_proxy() {
        let vars = env(&[
            ("HTTPS_PROXY", "ftp://p.test:1"),
            ("ALL_PROXY", "http://all.test:1"),
        ]);
        assert!(matches!(
            route(&vars, &url("https://a.example/")),
            Route::Proxy {
                source: Source::Variable("ALL_PROXY"),
                ..
            }
        ));
        let vars = env(&[("HTTPS_PROXY", "/x"), ("NO_PROXY", "other.test")]);
        assert_eq!(route(&vars, &url("https://a.example/")), Route::Direct);
    }

    #[test]
    fn the_system_settings_come_before_all_proxy_as_in_uv() {
        let vars = env(&[("ALL_PROXY", "http://all.test:1")]);
        let https = route_with_system(&vars, &url("https://a.example/"), "http://sys.test:8080");
        assert_eq!(
            https,
            Route::Proxy {
                url: "http://sys.test:8080/".into(),
                source: Source::SystemSettings
            }
        );
        // The settings have no HTTP proxy, so `ALL_PROXY` gives it.
        let http = route_with_system(&vars, &url("http://a.example/"), "http://sys.test:8080");
        assert!(matches!(
            http,
            Route::Proxy {
                source: Source::Variable("ALL_PROXY"),
                ..
            }
        ));
    }

    #[test]
    fn the_first_variable_that_is_set_wins_also_when_it_is_empty() {
        let target = url("http://127.0.0.1:8080/mcp");
        let vars = env(&[("HTTP_PROXY", ""), ("http_proxy", "http://p.test:1")]);
        assert_eq!(route(&vars, &target), Route::Direct);
        let vars = env(&[
            ("HTTP_PROXY", "http://p.test:1"),
            ("NO_PROXY", ""),
            ("no_proxy", "127.0.0.1"),
        ]);
        assert!(matches!(route(&vars, &target), Route::Proxy { .. }));
    }

    #[test]
    fn cgi_program_uses_no_proxy_for_any_scheme() {
        let vars = env(&[
            ("HTTPS_PROXY", "http://p.test:1"),
            ("REQUEST_METHOD", "GET"),
        ]);
        assert_eq!(route(&vars, &url("https://a.example/")), Route::Direct);
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
