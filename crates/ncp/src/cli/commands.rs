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
    args::{CallArgs, Cli, PromptCommand, ResourceCommand, ServerCommand, ToolCommand},
    outcome::{Exit, Failure},
    render,
    schema_check::{self, Outcome},
    session::{self, Context, Loaded},
    tool_args::{self, Argument, Given, Input, Resolved},
    ui,
};
use crate::{
    client::{Answer, Connection, RequestError},
    config::{EntryError, Server},
    list_cache,
};

pub async fn server(context: &Context<'_>, command: ServerCommand) -> Result<(), Failure> {
    match command {
        ServerCommand::List => server_list(context),
        ServerCommand::Show { server } => server_show(context, &server).await,
    }
}

pub async fn resource(context: &Context<'_>, command: ResourceCommand) -> Result<(), Failure> {
    match command {
        ResourceCommand::List { server } => resource_list(context, &server).await,
        ResourceCommand::Read { server, uri } => resource_read(context, &server, &uri).await,
    }
}

pub async fn prompt(context: &Context<'_>, command: PromptCommand) -> Result<(), Failure> {
    match command {
        PromptCommand::List { server } => prompt_list(context, &server).await,
        PromptCommand::Get {
            server,
            prompt,
            arguments,
        } => prompt_get(context, &server, &prompt, &arguments).await,
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
    match connection.list_tools_fresh_for(context.limits()).await {
        Ok((tools, hints)) => {
            let used_credentials = connection.is_authorized() || !server.value_names().is_empty();
            let keep = list_cache::keep_for(hints.ttl_ms, hints.private, used_credentials);
            keep_tools(context, server, &tools, keep);
            Ok(tools)
        }
        Err(error) => {
            super::auth::remember_scopes(context, server, &error).await;
            Err(session::request_failure(server, "tools/list", &error))
        }
    }
}

/// Keeps the tools in the cache for the time of `list_cache::keep_for`.
/// A list that `ncp` must not keep removes the old file. A cache that
/// cannot be written gives a warning: `ncp` works the same with no cache.
fn keep_tools(context: &Context<'_>, server: &Server, tools: &[Tool], ttl_ms: Option<u64>) {
    if !context.cache.writes() {
        return;
    }
    let path = list_cache::path(&context.paths.cache_dir, &server.name);
    match ttl_ms {
        Some(ttl_ms) => {
            let cached = list_cache::CachedTools {
                target: server.target.clone(),
                entry: list_cache::entry_digest(server),
                fetched_at_ms: list_cache::now_ms(),
                ttl_ms,
                tools: tools.to_vec(),
            };
            if let Err(error) = list_cache::save(&path, &cached) {
                context.ui.warning(
                    None,
                    &format!("cannot write the list cache {}: {error}", path.display()),
                );
            }
        }
        None => list_cache::remove(&path),
    }
}

/// The tools of the cache when they are fresh, so that the command needs
/// no server start.
fn cached_tools(context: &Context<'_>, server: &Server) -> Option<Vec<Tool>> {
    if !context.cache.reads() {
        return None;
    }
    let cached = list_cache::load(&list_cache::path(&context.paths.cache_dir, &server.name))?;
    let now = list_cache::now_ms();
    let tools = cached
        .fresh_for(&server.target, list_cache::entry_digest(server), now)?
        .to_vec();
    context.ui.debug(
        Some(server.name.as_str()),
        &format!(
            "tools from the list cache, {} s old",
            cached.age(now).as_secs()
        ),
    );
    Some(tools)
}

async fn tool_list(context: &Context<'_>, word: &str) -> Result<(), Failure> {
    let loaded = load(context)?;
    let server = select(context, &loaded, word, show_fix)?;
    if let Some(tools) = cached_tools(context, server) {
        let name = server.name.as_str();
        return context.ui.result(
            || render::tool_list(&tools),
            || json!({"server": name, "tools": tools}),
        );
    }
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
    let near = super::near::nearest(name, tools.iter().map(|tool| tool.name.as_ref()))
        .map(|near| format!("; did you mean {near:?}?"))
        .unwrap_or_default();
    let message = format!("the server has no tool {name:?}{near}");
    tools
        .into_iter()
        .find(|tool| tool.name == name)
        .ok_or_else(|| {
            Failure::usage(message)
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
    // A cached list that misses the tool gets a live list: the server can
    // have changed within the time of the list.
    let cached =
        cached_tools(context, server).and_then(|tools| find_tool(name, tools, tool_name).ok());
    let tool = match cached {
        Some(tool) => tool,
        None => {
            with_connection(context, server, async |connection| {
                find_tool(name, list(context, server, connection).await?, tool_name)
            })
            .await?
        }
    };
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
        Input::Stdin => Resolved::Object(read_stdin_object(context).await?),
        Input::Pairs(pairs) => Resolved::Pairs(read_files(pairs)?),
        Input::Object(object) => Resolved::Object(object),
    };
    let needs_tool = call.help || !call.no_validate || tool_args::needs_schema(&input);
    let name = server.name.as_str();
    with_connection(context, server, async |connection| {
        // rmcp mirrors parameters only for a tool that it listed in this
        // session, so that case lists again; else a fresh cache serves.
        let cached = if needs_tool && !mirrors_parameters(server, connection) {
            cached_tools(context, server)
        } else {
            None
        };
        let prepared = match cached {
            Some(tools) => {
                // The server can have changed within the time of the list
                // (the spec permits a fetch again): a list from the cache
                // that does not fit gets one live list.
                if let Ok(prepared) =
                    prepare_call(context, name, Some(tools), &input, tool_name, &call)
                {
                    prepared
                } else {
                    let tools = list(context, server, connection).await?;
                    prepare_call(context, name, Some(tools), &input, tool_name, &call)?
                }
            }
            None if needs_tool || mirrors_parameters(server, connection) => {
                let tools = list(context, server, connection).await?;
                let tools = needs_tool.then_some(tools);
                prepare_call(context, name, tools, &input, tool_name, &call)?
            }
            None => prepare_call(context, name, None, &input, tool_name, &call)?,
        };
        let (tool, arguments) = prepared;
        if let (true, Some(tool)) = (call.help, &tool) {
            return show_tool(context, name, tool);
        }
        let response = connection
            .call_tool(tool_name, arguments, context.limits())
            .await;
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                super::auth::remember_scopes(context, server, &error).await;
                return Err(call_failure(server, tool_name, tool.is_some(), &error));
            }
        };
        print_response(context, name, tool_name, response)
    })
    .await
}

/// The tool and the arguments of a call: the tool from the list, the
/// arguments with the schema rule, and the schema check unless
/// `--no-validate`.
fn prepare_call(
    context: &Context<'_>,
    server: &str,
    tools: Option<Vec<Tool>>,
    input: &Resolved,
    tool_name: &str,
    call: &CallArgs,
) -> Result<(Option<Tool>, JsonObject), Failure> {
    let tool = tools
        .map(|tools| find_tool(server, tools, tool_name))
        .transpose()?;
    if call.help {
        return Ok((tool, JsonObject::new()));
    }
    let arguments = match &tool {
        Some(tool) => tool_args::coerce(input, &tool.input_schema),
        None => tool_args::coerce(input, &JsonObject::new()),
    };
    if let (false, Some(tool)) = (call.no_validate, &tool) {
        check_arguments(context, server, tool, &arguments)?;
    }
    Ok((tool, arguments))
}

/// True when a call needs the tool list first: from `2026-07-28` an HTTP
/// client mirrors the parameters with `x-mcp-header` into `Mcp-Param-*`
/// headers, and rmcp knows these parameters only from a listed tool
/// (SEP-2243). A tool that the list does not show stays callable.
fn mirrors_parameters(server: &Server, connection: &Connection) -> bool {
    matches!(server.transport, crate::config::Transport::Http(_))
        && matches!(connection.era(), crate::era::Era::Modern { .. })
}

/// True when the server says in its handshake that it has the feature.
fn offers(
    connection: &Connection,
    feature: impl Fn(&rmcp::model::ServerCapabilities) -> bool,
) -> bool {
    connection
        .server_info()
        .is_some_and(|info| feature(&info.capabilities))
}

/// A list request that a server can lack: "method not found" gives an
/// empty list, because the list is a part of a larger answer.
fn empty_when_missing<T>(result: Result<Vec<T>, RequestError>) -> Result<Vec<T>, RequestError> {
    match result {
        Err(RequestError::Service(ServiceError::McpError(data)))
            if data.code == ErrorCode::METHOD_NOT_FOUND =>
        {
            Ok(Vec::new())
        }
        other => other,
    }
}

async fn resource_list(context: &Context<'_>, word: &str) -> Result<(), Failure> {
    let loaded = load(context)?;
    let server = select(context, &loaded, word, show_fix)?;
    let name = server.name.as_str();
    let found = with_connection(context, server, async |connection| {
        if !offers(connection, |capabilities| capabilities.resources.is_some()) {
            return Ok(None);
        }
        let resources = connection
            .list_resources(context.limits())
            .await
            .map_err(|error| session::request_failure(server, "resources/list", &error))?;
        let templates =
            empty_when_missing(connection.list_resource_templates(context.limits()).await)
                .map_err(|error| {
                    session::request_failure(server, "resources/templates/list", &error)
                })?;
        Ok(Some((resources, templates)))
    })
    .await?;
    let (resources, templates) = found.unwrap_or_else(|| {
        context.ui.hint(
            Some(name),
            "the server offers no resources in its handshake",
        );
        (Vec::new(), Vec::new())
    });
    let resources = serde_json::to_value(&resources).unwrap_or(Value::Null);
    let templates = serde_json::to_value(&templates).unwrap_or(Value::Null);
    let list = |value: &Value| value.as_array().cloned().unwrap_or_default();
    context.ui.result(
        || render::resource_list(&list(&resources), &list(&templates)),
        || json!({"server": name, "resources": resources, "resourceTemplates": templates}),
    )
}

/// The failure of a request that names a thing by name or URI: "invalid
/// params" or "resource not found" is a usage error with the command that
/// lists the names.
fn named_failure(server: &Server, method: &str, error: &RequestError, list: &str) -> Failure {
    let name = server.name.as_str();
    match error {
        RequestError::Service(ServiceError::McpError(data))
            if data.code == ErrorCode::INVALID_PARAMS
                || data.code == ErrorCode::RESOURCE_NOT_FOUND =>
        {
            Failure::usage(format!("{method}: the server refused it: {}", data.message))
                .server(name)
                .hint(format!("ncp {list} list {}", render::shell_word(name)))
        }
        _ => session::request_failure(server, method, error),
    }
}

async fn resource_read(context: &Context<'_>, word: &str, uri: &str) -> Result<(), Failure> {
    let loaded = load(context)?;
    let server = select(context, &loaded, word, show_fix)?;
    let name = server.name.as_str();
    let answer = with_connection(context, server, async |connection| {
        connection
            .read_resource(uri, context.limits())
            .await
            .map_err(|error| named_failure(server, "resources/read", &error, "resource"))
    })
    .await?;
    let result = match answer {
        Answer::Complete(result) => result,
        Answer::InputRequired(result) => {
            return input_required(context, name, "the resource", &result);
        }
    };
    let value = serde_json::to_value(&result).unwrap_or(Value::Null);
    // A pipe gets JSON, as from each command. `--output-format text` in a
    // pipe gives the contents as they are, for example into a file.
    if context.ui.format == ui::Format::Text && !std::io::stdout().is_terminal() {
        return ui::write_stdout_bytes(&raw_contents(&value)?);
    }
    context
        .ui
        .result(|| render::resource_contents(&value), || value.clone())
}

/// The one content of a resource as bytes: the text, or the decoded blob.
/// Several contents have no raw form: their borders and types would be
/// lost.
fn raw_contents(result: &Value) -> Result<Vec<u8>, Failure> {
    let contents = result
        .get("contents")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let [content] = contents else {
        return Err(Failure::usage(format!(
            "the resource has {} contents, and text output gives one",
            contents.len()
        ))
        .hint("use --output-format json"));
    };
    if let Some(text) = content.get("text").and_then(Value::as_str) {
        return Ok(text.as_bytes().to_vec());
    }
    let blob = content
        .get("blob")
        .and_then(Value::as_str)
        .unwrap_or_default();
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, blob).map_err(|error| {
        Failure::protocol(format!(
            "the server sent a blob that is not base64: {error}"
        ))
    })
}

/// An `input_required` result: on stdout, with exit 5, as for a tool call.
fn input_required(
    context: &Context<'_>,
    server: &str,
    what: &str,
    result: &rmcp::model::InputRequiredResult,
) -> Result<(), Failure> {
    let value = serde_json::to_value(result).unwrap_or(Value::Null);
    context
        .ui
        .result(|| format!("{}\n", render::pretty(&value)), || value.clone())?;
    Err(Failure::new(
        Exit::InputRequired,
        format!("{what} asks for more input, and ncp cannot give it yet"),
    )
    .server(server))
}

async fn prompt_list(context: &Context<'_>, word: &str) -> Result<(), Failure> {
    let loaded = load(context)?;
    let server = select(context, &loaded, word, show_fix)?;
    let name = server.name.as_str();
    let prompts = with_connection(context, server, async |connection| {
        if !offers(connection, |capabilities| capabilities.prompts.is_some()) {
            return Ok(None);
        }
        connection
            .list_prompts(context.limits())
            .await
            .map(Some)
            .map_err(|error| session::request_failure(server, "prompts/list", &error))
    })
    .await?
    .unwrap_or_else(|| {
        context
            .ui
            .hint(Some(name), "the server offers no prompts in its handshake");
        Vec::new()
    });
    let prompts = serde_json::to_value(&prompts).unwrap_or(Value::Null);
    let list = prompts.as_array().cloned().unwrap_or_default();
    context.ui.result(
        || render::prompt_list(&list),
        || json!({"server": name, "prompts": prompts}),
    )
}

/// The arguments of a prompt: text only, as the spec gives them.
fn prompt_arguments(words: &[String]) -> Result<JsonObject, Failure> {
    let pairs = match tool_args::parse(words).map_err(|error| Failure::usage(error.to_string()))? {
        Input::Pairs(pairs) => read_files(pairs)?,
        _ => {
            return Err(Failure::usage(
                "prompt arguments are key=value or key=@path pairs",
            ));
        }
    };
    pairs
        .into_iter()
        .map(|(key, argument)| match argument {
            Argument::Text(text) => Ok((key, Value::String(text))),
            Argument::Json(_) => Err(Failure::usage(format!(
                "prompt argument {key:?} must be text: use {key}=value"
            ))),
        })
        .collect()
}

async fn prompt_get(
    context: &Context<'_>,
    word: &str,
    prompt: &str,
    words: &[String],
) -> Result<(), Failure> {
    let arguments = prompt_arguments(words)?;
    let loaded = load(context)?;
    let server = select(context, &loaded, word, show_fix)?;
    let name = server.name.as_str();
    let answer = with_connection(context, server, async |connection| {
        connection
            .get_prompt(prompt, arguments.clone(), context.limits())
            .await
            .map_err(|error| named_failure(server, "prompts/get", &error, "prompt"))
    })
    .await?;
    match answer {
        Answer::Complete(result) => {
            let value = serde_json::to_value(&result).unwrap_or(Value::Null);
            context
                .ui
                .result(|| render::prompt_messages(&value), || value.clone())
        }
        Answer::InputRequired(result) => input_required(context, name, "the prompt", &result),
    }
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
/// That is a usage error. When `ncp` knows the tool from the list, `tool
/// show` gives the fix; else the tool can be unknown, and `tool list`
/// gives the fix.
fn call_failure(server: &Server, tool: &str, listed: bool, error: &RequestError) -> Failure {
    let name = server.name.as_str();
    match error {
        RequestError::Service(ServiceError::McpError(data))
            if data.code == ErrorCode::INVALID_PARAMS =>
        {
            let hint = if listed {
                format!(
                    "ncp tool show {} {}",
                    render::shell_word(name),
                    render::shell_word(tool)
                )
            } else {
                format!("ncp tool list {}", render::shell_word(name))
            };
            Failure::usage(format!("the server refused the call: {}", data.message))
                .server(name)
                .hint(hint)
        }
        _ => session::request_failure(server, "tools/call", error),
    }
}

/// The largest file of a `key=@path` pair. An argument is a message to a
/// server, not a bulk transfer, and a device such as `/dev/zero` has no end.
const MAX_ARGUMENT_FILE: u64 = 1024 * 1024;

/// Reads the file of each `key=@path` pair. Its text is the value, as for
/// `key=value`.
fn read_files(pairs: Vec<(String, Given)>) -> Result<Vec<(String, Argument)>, Failure> {
    pairs
        .into_iter()
        .map(|(key, given)| match given {
            Given::Text(text) => Ok((key, Argument::Text(text))),
            Given::Json(value) => Ok((key, Argument::Json(value))),
            Given::File(path) => {
                read_argument_file(&key, &path).map(|text| (key, Argument::Text(text)))
            }
        })
        .collect()
}

/// A regular file of at most the limit: a pipe or a device can block or
/// have no end.
fn read_argument_file(key: &str, path: &str) -> Result<String, Failure> {
    use std::io::Read as _;
    let failed = |reason: String| Failure::usage(format!("{key}=@{path}: {reason}"));
    if path.is_empty() {
        return Err(Failure::usage(format!("{key}=@ names no file"))
            .hint(format!("{key}=@PATH, or {key}=@@ for a literal @")));
    }
    // One open: the checks and the read are of the same file.
    let file = std::fs::File::open(path)
        .map_err(|error| failed(format!("cannot read the file: {error}")))?;
    let metadata = file
        .metadata()
        .map_err(|error| failed(format!("cannot read the file: {error}")))?;
    if !metadata.is_file() {
        return Err(failed("is not a regular file".to_string()));
    }
    let mut text = String::new();
    file.take(MAX_ARGUMENT_FILE + 1)
        .read_to_string(&mut text)
        .map_err(|error| failed(format!("cannot read the file: {error}")))?;
    if text.len() as u64 > MAX_ARGUMENT_FILE {
        return Err(failed(format!(
            "the file is larger than {MAX_ARGUMENT_FILE} bytes"
        )));
    }
    Ok(text)
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
