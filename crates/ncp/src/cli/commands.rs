//! The commands. Each one reads its inputs, does its I/O, and gives its
//! result to the UI. The pure parts are in the other modules.

use std::{fmt::Write as _, io::IsTerminal};

use clap::CommandFactory;
use rmcp::{
    ServiceError,
    model::{CallToolResponse, ErrorCode, JsonObject, Tool},
};
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;

use super::{
    args::{CallArgs, Cli, ServerCommand, ToolCommand},
    outcome::{Exit, Failure},
    render,
    schema_check::{self, Outcome},
    session::{self, Context, Loaded},
    tool_args::{self, Input},
    ui,
};
use crate::{
    client::{Connection, RequestError},
    config::{EntryError, Server},
};

pub async fn server(context: &Context<'_>, command: ServerCommand) -> Result<(), Failure> {
    match command {
        ServerCommand::List => server_list(context),
        ServerCommand::Show { server } => server_show(context, &server).await,
    }
}

pub async fn tool(context: &Context<'_>, command: ToolCommand) -> Result<(), Failure> {
    match command {
        ToolCommand::List { server } => tool_list(context, &server).await,
        ToolCommand::Show { server, tool } => tool_show(context, &server, tool.as_deref()).await,
        ToolCommand::Call(call) => tool_call(context, call).await,
    }
}

fn load(context: &Context<'_>) -> Result<Loaded, Failure> {
    let path = &context.paths.config_file;
    let loaded = context.timed(
        |record, elapsed| record.config = Some(elapsed),
        || session::load_config(path, context.env),
    )?;
    let state = if loaded.exists {
        ""
    } else {
        " (does not exist)"
    };
    context.ui.debug(
        None,
        &format!("configuration file: {}{state}", path.display()),
    );
    Ok(loaded)
}

fn server_list(context: &Context<'_>) -> Result<(), Failure> {
    let loaded = load(context)?;
    let config = &loaded.config;
    for warning in config.warnings() {
        context.ui.warning(
            warning.server.as_deref(),
            &session::unknown_key(&warning.key),
        );
    }
    let servers: Vec<&Server> = config.servers().collect();
    let invalid: Vec<(&str, &EntryError)> = config.invalid().collect();
    if servers.is_empty() && invalid.is_empty() {
        let path = loaded.path.display();
        let message = if loaded.exists {
            format!("no server is configured in {path}")
        } else {
            format!("no server is configured: {path} does not exist")
        };
        context.ui.hint(None, &message);
    }
    context.ui.result(
        || {
            let rows: Vec<Vec<String>> = servers
                .iter()
                .map(|server| {
                    vec![
                        server.name.to_string(),
                        render::transport_name(server).to_string(),
                        render::target(server),
                    ]
                })
                .chain(invalid.iter().map(|(name, error)| {
                    vec![(*name).to_string(), "invalid".into(), error.to_string()]
                }))
                .collect();
            render::table(&rows)
        },
        || {
            json!({
                "servers": servers.iter().map(|server| json!({
                    "name": server.name.as_str(),
                    "transport": render::transport_name(server),
                    "target": render::target(server),
                })).collect::<Vec<_>>(),
                "invalid": invalid.iter().map(|(name, error)| json!({
                    "name": name,
                    "error": error.to_string(),
                })).collect::<Vec<_>>(),
            })
        },
    )?;
    Ok(())
}

/// Selects the server named by `word`. A joined word gives the fix.
fn select<'a>(
    context: &Context<'_>,
    loaded: &'a Loaded,
    word: &str,
    fix: impl FnOnce(&str, &str) -> String,
) -> Result<&'a Server, Failure> {
    if let Some((server, tool)) = session::split_joined(word) {
        return Err(session::joined_failure(word, &fix(server, tool)));
    }
    session::select(loaded, word, context.ui)
}

fn show_fix(server: &str, tool: &str) -> String {
    format!(
        "ncp tool show {} {}",
        render::shell_word(server),
        render::shell_word(tool)
    )
}

/// Runs `work` on a new connection, and always closes the connection. A
/// server can let a connect with no token in and answer `401` to a
/// request; then `work` runs once more on a connection with the stored
/// login. The server did not do the first request.
async fn with_connection<T>(
    context: &Context<'_>,
    server: &Server,
    work: impl AsyncFn(&Connection) -> Result<T, Failure>,
) -> Result<T, Failure> {
    let connection = session::open(context, server).await?;
    let result = work(&connection).await;
    let authorized = connection.is_authorized();
    connection.close().await;
    match result {
        Err(Failure {
            login_challenge: Some(challenge),
            ..
        }) if !authorized => {
            let connection = session::open_with_login(context, server, Some(&challenge)).await?;
            let result = work(&connection).await;
            connection.close().await;
            result
        }
        other => other,
    }
}

async fn server_show(context: &Context<'_>, word: &str) -> Result<(), Failure> {
    let loaded = load(context)?;
    let server = select(context, &loaded, word, show_fix)?;
    let info = with_connection(context, server, async |connection| {
        Ok(connection.server_info())
    })
    .await?;
    let peer = info
        .and_then(|info| serde_json::to_value(&*info).ok())
        .unwrap_or(Value::Null);
    let mut show = JsonObject::new();
    show.insert("server".into(), json!(server.name.as_str()));
    show.insert("target".into(), json!(render::target(server)));
    for key in [
        "protocolVersion",
        "serverInfo",
        "capabilities",
        "instructions",
    ] {
        if let Some(value) = peer.get(key) {
            show.insert(key.into(), value.clone());
        }
    }
    context.ui.result(
        || render::server_show(&show),
        || Value::Object(show.clone()),
    )?;
    Ok(())
}

async fn list(
    context: &Context<'_>,
    server: &Server,
    connection: &Connection,
) -> Result<Vec<Tool>, Failure> {
    match connection.list_tools(context.limits()).await {
        Ok(tools) => Ok(tools),
        Err(error) => {
            super::auth::remember_scopes(context, server, &error).await;
            Err(session::request_failure(server, "tools/list", &error))
        }
    }
}

async fn tool_list(context: &Context<'_>, word: &str) -> Result<(), Failure> {
    let loaded = load(context)?;
    let server = select(context, &loaded, word, show_fix)?;
    let (tools, has_instructions) = with_connection(context, server, async |connection| {
        let tools = list(context, server, connection).await?;
        let has_instructions = connection
            .server_info()
            .is_some_and(|info| info.instructions.is_some());
        Ok((tools, has_instructions))
    })
    .await?;
    let name = server.name.as_str();
    if has_instructions {
        context.ui.hint(
            Some(name),
            &format!("the server has instructions for clients: ncp server show {name}"),
        );
    }
    context.ui.result(
        || render::tool_list(&tools),
        || json!({"server": name, "tools": tools}),
    )?;
    Ok(())
}

fn find_tool(server: &str, tools: Vec<Tool>, name: &str) -> Result<Tool, Failure> {
    tools
        .into_iter()
        .find(|tool| tool.name == name)
        .ok_or_else(|| {
            Failure::usage(format!("the server has no tool {name:?}"))
                .server(server)
                .hint(format!("ncp tool list {}", render::shell_word(server)))
        })
}

/// The tool word, or the failure that names the fix.
fn tool_word<'a>(server: &str, tool: Option<&'a str>) -> Result<&'a str, Failure> {
    let missing = |message: String| {
        Failure::usage(message).hint(format!("ncp tool list {}", render::shell_word(server)))
    };
    match tool {
        None => Err(missing("missing tool name".into())),
        Some(word) if word.contains('=') || word.starts_with('{') || word == "-" => Err(missing(
            format!("missing tool name: {word:?} is a tool argument"),
        )),
        Some(word) => Ok(word),
    }
}

fn show_tool(context: &Context<'_>, server: &str, tool: &Tool) -> Result<(), Failure> {
    context.ui.result(
        || render::tool_show(server, tool),
        || json!({"server": server, "tool": tool}),
    )
}

async fn tool_show(context: &Context<'_>, word: &str, tool: Option<&str>) -> Result<(), Failure> {
    let loaded = load(context)?;
    let server = select(context, &loaded, word, show_fix)?;
    let name = server.name.as_str();
    let tool_name = tool_word(name, tool)?;
    let tool = with_connection(context, server, async |connection| {
        find_tool(name, list(context, server, connection).await?, tool_name)
    })
    .await?;
    show_tool(context, name, &tool)
}

async fn tool_call(context: &Context<'_>, call: CallArgs) -> Result<(), Failure> {
    let word = match (call.server.as_deref(), call.tool.as_deref(), call.help) {
        (None, _, true) | (Some(_), None, true) => return print_call_help(),
        (None, _, false) => {
            return Err(Failure::usage("missing server name").hint("ncp server list"));
        }
        (Some(word), _, _) => word,
    };
    let call_fix = |server: &str, tool: &str| {
        std::iter::once(format!(
            "ncp tool call {} {}",
            render::shell_word(server),
            render::shell_word(tool)
        ))
        .chain(
            call.tool
                .iter()
                .chain(&call.arguments)
                .map(|w| render::shell_word(w)),
        )
        .collect::<Vec<_>>()
        .join(" ")
    };
    if let Some((server, tool)) = session::split_joined(word) {
        return Err(session::joined_failure(word, &call_fix(server, tool)));
    }
    let tool_name = tool_word(word, call.tool.as_deref())?;
    let input = if call.help {
        Input::Pairs(Vec::new())
    } else {
        tool_args::parse(&call.arguments).map_err(|error| Failure::usage(error.to_string()))?
    };
    let loaded = load(context)?;
    let server = session::select(&loaded, word, context.ui)?;
    let input = match input {
        Input::Stdin => Input::Object(read_stdin_object(context).await?),
        other => other,
    };
    let needs_tool = call.help
        || !call.no_validate
        || matches!(&input, Input::Pairs(pairs) if !pairs.is_empty());
    let name = server.name.as_str();
    with_connection(context, server, async |connection| {
        let tools = if needs_tool || mirrors_parameters(server, connection) {
            Some(list(context, server, connection).await?)
        } else {
            None
        };
        let tool = match tools {
            Some(tools) if needs_tool => Some(find_tool(name, tools, tool_name)?),
            _ => None,
        };
        if let (true, Some(tool)) = (call.help, &tool) {
            return show_tool(context, name, tool);
        }
        let arguments = match (&input, &tool) {
            (Input::Pairs(pairs), Some(tool)) => tool_args::coerce(pairs, &tool.input_schema),
            (Input::Object(object), _) => object.clone(),
            _ => JsonObject::new(),
        };
        if let (false, Some(tool)) = (call.no_validate, &tool) {
            check_arguments(context, name, tool, &arguments)?;
        }
        let response = connection
            .call_tool(tool_name, arguments, context.limits())
            .await;
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                super::auth::remember_scopes(context, server, &error).await;
                return Err(call_failure(server, tool_name, &error));
            }
        };
        print_response(context, name, tool_name, response)
    })
    .await
}

/// True when a call needs the tool list first: from `2026-07-28` an HTTP
/// client mirrors the parameters with `x-mcp-header` into `Mcp-Param-*`
/// headers, and rmcp knows these parameters only from a listed tool
/// (SEP-2243). A tool that the list does not show stays callable.
fn mirrors_parameters(server: &Server, connection: &Connection) -> bool {
    matches!(server.transport, crate::config::Transport::Http(_))
        && matches!(connection.era(), crate::era::Era::Modern { .. })
}

fn print_call_help() -> Result<(), Failure> {
    // A built command has the full name and the global options in its help.
    let mut command = Cli::command();
    command.build();
    let help = command
        .find_subcommand_mut("tool")
        .and_then(|tool| tool.find_subcommand_mut("call"))
        .map(|call| call.render_long_help().to_string())
        .unwrap_or_default();
    ui::write_stdout(&help)
}

/// The spec says that a server answers an unknown tool or bad arguments
/// with "invalid params" (MCP 2025-11-25, server/tools, "Error Handling").
/// That is a usage error, and `tool show` gives the fix.
fn call_failure(server: &Server, tool: &str, error: &RequestError) -> Failure {
    let name = server.name.as_str();
    match error {
        RequestError::Service(ServiceError::McpError(data))
            if data.code == ErrorCode::INVALID_PARAMS =>
        {
            Failure::usage(format!("the server refused the call: {}", data.message))
                .server(name)
                .hint(format!(
                    "ncp tool show {} {}",
                    render::shell_word(name),
                    render::shell_word(tool)
                ))
        }
        _ => session::request_failure(server, "tools/call", error),
    }
}

async fn read_stdin_object(context: &Context<'_>) -> Result<JsonObject, Failure> {
    if std::io::stdin().is_terminal() {
        context.ui.hint(
            None,
            "ncp reads the JSON arguments from stdin; end the input with Ctrl-D",
        );
    }
    let mut text = String::new();
    let mut stdin = tokio::io::stdin();
    tokio::select! {
        read = stdin.read_to_string(&mut text) => read.map_err(|error| {
            Failure::usage(format!("cannot read the arguments from stdin: {error}"))
        })?,
        () = context.cancel.cancelled() => return Err(session::interrupted()),
    };
    tool_args::object(&text).map_err(|error| Failure::usage(error.to_string()))
}

fn check_arguments(
    context: &Context<'_>,
    server: &str,
    tool: &Tool,
    arguments: &JsonObject,
) -> Result<(), Failure> {
    match schema_check::check(&tool.input_schema, arguments) {
        Outcome::Valid => Ok(()),
        Outcome::SchemaNotUsable(reason) => {
            context.ui.warning(
                Some(server),
                &format!(
                    "ncp cannot use the input schema of tool {} ({reason}); the arguments are not checked",
                    tool.name
                ),
            );
            Ok(())
        }
        Outcome::Invalid(problems) => {
            let problems = problems.iter().fold(String::new(), |mut text, problem| {
                let path = if problem.path.is_empty() {
                    "/"
                } else {
                    &problem.path
                };
                let _ = write!(text, "\n  {path}: {}", problem.message);
                text
            });
            Err(Failure::usage(format!(
                "the arguments do not match the input schema of tool {}:{problems}\nExpected arguments:\n{}",
                tool.name,
                render::properties(&tool.input_schema).trim_end()
            ))
            .server(server)
            .hint(render::example_call(server, tool)))
        }
    }
}

fn print_response(
    context: &Context<'_>,
    server: &str,
    tool: &str,
    response: CallToolResponse,
) -> Result<(), Failure> {
    let failure = |exit: Exit, message: String| Err(Failure::new(exit, message).server(server));
    match response {
        CallToolResponse::Complete(result) => {
            let value = serde_json::to_value(&result).unwrap_or(Value::Null);
            context
                .ui
                .result(|| render::call_result(&value), || value.clone())?;
            if result.is_error == Some(true) {
                failure(Exit::Tool, format!("tool {tool} returned an error"))
            } else {
                Ok(())
            }
        }
        CallToolResponse::InputRequired(result) => {
            let value = serde_json::to_value(&result).unwrap_or(Value::Null);
            context
                .ui
                .result(|| format!("{}\n", render::pretty(&value)), || value.clone())?;
            failure(
                Exit::InputRequired,
                format!("tool {tool} asks for more input, and ncp cannot give it yet"),
            )
        }
        // `ncp` asks for no task, so a task result is a protocol error.
        _ => failure(
            Exit::Protocol,
            format!("tool {tool} gave a result that ncp did not ask for"),
        ),
    }
}
