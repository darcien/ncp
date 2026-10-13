//! Where a stored value can go. Each `${NAME}` in a field that can read the
//! store is a use, and each use has the destination of its field. A stored
//! value goes only to the destination in its key, so these pure functions
//! decide each key that `ncp` reads or writes.

use crate::{
    config::{self, PreRegisteredClient, Server, Transport},
    credential::Destination,
    server::{Origin, OriginError},
};

/// The field of a use: the two fields have different destinations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// An `env` value of a stdio entry, or a `headers` value.
    Value,
    /// `oauth.clientSecret`. It goes only to the issuer.
    ClientSecret,
}

/// One entry that names `${NAME}` in a field that can read the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Use<'a> {
    pub server: &'a Server,
    pub field: Field,
    pub destination: Result<Destination, OriginError>,
}

/// A stdio entry has no destination. An http entry has the origin of the
/// URL as the file writes it.
///
/// # Errors
///
/// Returns an error when the scheme, host or port of the URL is not
/// literal text.
pub fn value_destination(server: &Server) -> Result<Destination, OriginError> {
    match &server.transport {
        Transport::Stdio(_) => Ok(Destination::Stdio),
        Transport::Http(remote) | Transport::Sse(remote) => {
            let written = match &server.target {
                config::Target::Remote { url } => url.as_str(),
                config::Target::Stdio { .. } => "",
            };
            Origin::of_written(written, remote.url.as_url()).map(Destination::Origin)
        }
    }
}

/// The destination of a client secret: the origin of `oauth.issuer`. The
/// issuer is literal text in the file, and `ncp` checks the issuer of the
/// metadata against it before each send (design B68).
///
/// # Errors
///
/// Returns an error when the issuer is not an `https` or `http` URL.
pub fn issuer_destination(issuer: &str) -> Result<Destination, OriginError> {
    url::Url::parse(issuer)
        .map_err(|_| OriginError::NotHttp(issuer.to_string()))
        .and_then(|url| Origin::of_written(issuer, &url))
        .map(Destination::Origin)
}

/// Each use of `name` in the entries, in the order of the entries.
pub fn uses<'a>(servers: impl Iterator<Item = &'a Server>, name: &str) -> Vec<Use<'a>> {
    servers
        .flat_map(|server| {
            let value = server.value_names().contains(name).then(|| Use {
                server,
                field: Field::Value,
                destination: value_destination(server),
            });
            let secret = client_secret_use(server, name);
            value.into_iter().chain(secret)
        })
        .collect()
}

fn client_secret_use<'a>(server: &'a Server, name: &str) -> Option<Use<'a>> {
    let (Transport::Http(remote) | Transport::Sse(remote)) = &server.transport else {
        return None;
    };
    let client: &PreRegisteredClient = remote.oauth_client.as_ref()?;
    let issuer = client.issuer()?;
    client
        .secret()
        .filter(|secret| secret.names().contains(&name))
        .map(|_| Use {
            server,
            field: Field::ClientSecret,
            destination: issuer_destination(issuer),
        })
}

/// One destination and the uses that share it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target<'a> {
    pub destination: Destination,
    pub uses: Vec<Use<'a>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetError<'a> {
    /// No entry names `${NAME}` in a field that can read the store.
    NoUse,
    /// An entry that names `${NAME}` has no literal origin, so no stored
    /// value can go to it.
    NotLiteral {
        server: &'a Server,
        error: OriginError,
    },
    /// The uses have different destinations: the user must choose one.
    Several(Vec<Target<'a>>),
}

/// The one destination of the uses: the key that `secret set` writes.
///
/// # Errors
///
/// Returns an error when there is no use, a use with no literal origin, or
/// more than one destination.
pub fn target(uses: Vec<Use<'_>>) -> Result<Target<'_>, TargetError<'_>> {
    if let Some(Use {
        server,
        destination: Err(error),
        ..
    }) = uses.iter().find(|item| item.destination.is_err())
    {
        return Err(TargetError::NotLiteral {
            server,
            error: error.clone(),
        });
    }
    let mut targets = by_destination(uses);
    match targets.len() {
        0 => Err(TargetError::NoUse),
        1 => Ok(targets.remove(0)),
        _ => Err(TargetError::Several(targets)),
    }
}

/// The uses with a destination, grouped by it, in the order of the first
/// use of each destination.
#[must_use]
pub fn by_destination(uses: Vec<Use<'_>>) -> Vec<Target<'_>> {
    uses.into_iter()
        .fold(Vec::<Target>::new(), |mut targets, item| {
            let Ok(destination) = item.destination.clone() else {
                return targets;
            };
            match targets
                .iter_mut()
                .find(|target| target.destination == destination)
            {
                Some(target) => target.uses.push(item),
                None => targets.push(Target {
                    destination,
                    uses: vec![item],
                }),
            }
            targets
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expand::Env;

    fn config(json: &str) -> config::Config {
        config::parse(json, &Env::from([("P".into(), "x".into())])).unwrap()
    }

    fn servers<'a>(uses: &[Use<'a>]) -> Vec<&'a str> {
        uses.iter().map(|item| item.server.name.as_str()).collect()
    }

    const FILE: &str = r#"{"mcpServers": {
        "a": {"type": "http", "url": "https://h/a", "headers": {"X": "${K}"}},
        "b": {"type": "http", "url": "https://H:443/b/${P}", "headers": {"X": "k ${K}"}},
        "c": {"type": "http", "url": "https://other/mcp", "headers": {"X": "${K}"}},
        "d": {"command": "d", "env": {"K": "${K}"}},
        "e": {"type": "http", "url": "https://e/mcp", "oauth": {"clientId": "i",
              "clientSecret": "${K}", "issuer": "https://login.e/tenant"}},
        "f": {"type": "http", "url": "https://f${P}/mcp", "headers": {"X": "${K}"}},
        "g": {"command": "g", "args": ["${K}"]}
    }}"#;

    #[test]
    fn each_field_has_its_destination() {
        let config = config(FILE);
        let uses = uses(config.servers(), "K");
        assert_eq!(servers(&uses), ["a", "b", "c", "d", "e", "f"]);
        let destinations: Vec<String> = uses
            .iter()
            .map(|item| match &item.destination {
                Ok(destination) => destination.to_string(),
                Err(_) => "error".into(),
            })
            .collect();
        assert_eq!(
            destinations,
            [
                "https://h",
                "https://h",
                "https://other",
                "stdio servers",
                "https://login.e",
                "error"
            ]
        );
        assert_eq!(uses[4].field, Field::ClientSecret);
    }

    #[test]
    fn target_is_the_one_destination_of_the_uses() {
        let config = config(FILE);
        let only = |names: &[&str]| -> Vec<Use<'_>> {
            uses(config.servers(), "K")
                .into_iter()
                .filter(|item| names.contains(&item.server.name.as_str()))
                .collect()
        };
        let target = target(only(&["a", "b"])).unwrap();
        assert_eq!(target.destination.to_string(), "https://h");
        assert_eq!(servers(&target.uses), ["a", "b"]);
        let Err(TargetError::Several(targets)) = super::target(only(&["a", "c", "d"])) else {
            panic!("three destinations need a choice");
        };
        assert_eq!(targets.len(), 3);
        assert!(matches!(
            super::target(only(&["a", "f"])),
            Err(TargetError::NotLiteral { .. })
        ));
        assert_eq!(super::target(only(&["g"])), Err(TargetError::NoUse));
    }

    #[test]
    fn a_header_that_names_a_client_secret_has_the_origin_of_the_server() {
        let config = config(
            r#"{"mcpServers": {
            "e": {"type": "http", "url": "https://e/mcp", "oauth": {"clientId": "i",
                  "clientSecret": "${S}", "issuer": "https://login.e"}},
            "x": {"type": "http", "url": "https://login.e/mcp", "headers": {"X": "${S}"}},
            "y": {"type": "http", "url": "https://e/mcp", "headers": {"X": "${S}"}}
        }}"#,
        );
        let targets = by_destination(uses(config.servers(), "S"));
        let view: Vec<(String, Vec<&str>)> = targets
            .iter()
            .map(|target| (target.destination.to_string(), servers(&target.uses)))
            .collect();
        // The header of y goes to the MCP server, so it cannot read the
        // client secret of e. A header at the issuer origin can.
        assert_eq!(
            view,
            [
                ("https://login.e".to_string(), vec!["e", "x"]),
                ("https://e".to_string(), vec!["y"]),
            ]
        );
    }

    #[test]
    fn an_issuer_that_is_not_a_url_has_no_destination() {
        assert!(issuer_destination("login.e").is_err());
        assert!(issuer_destination("https://${HOST}").is_err());
    }
}
