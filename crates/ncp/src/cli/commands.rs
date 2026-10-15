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
    outcome::{Exit, Failure, chain_text},
    render,
    schema_check::{self, Outcome},
    session::{self, Context, Loaded},
    tool_args::{self, Input, Resolved},
    ui,
};
use crate::{
    client::{Answer, Connection, RequestError},
    config::Server,
    list_cache,
    project::Trust,
};

pub async fn server(context: &Context<'_>, command: ServerCommand) -> Result<(), Failure> {
    match command {
        ServerCommand::Add(args) => super::server_edit::add(context, args).await,
        ServerCommand::Remove { server, global } => {
            super::server_edit::remove(context, &server, global).await
        }
        ServerCommand::List => server_list(context).await,
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

async fn server_list(context: &Context<'_>) -> Result<(), Failure> {
    let loaded = session::load(context, None).await?;
    for warning in loaded.config.warnings() {
        context.ui.warning(
            warning.server.as_deref(),
            &session::unknown_key(&warning.key),
        );
    }
    let rows = list_rows(&loaded);
    if rows.is_empty() {
        let path = loaded.path.display();
        let message = if loaded.exists {
            format!("no server is configured in {path}")
        } else {
            format!("no server is configured: {path} does not exist")
        };
        context.ui.hint(None, &message);
    }
    let (servers, invalid) = render::rows_json(&rows);
    context.ui.result(
        || render::server_list(loaded.project.project(), &rows),
        || json!({"servers": servers, "invalid": invalid}),
    )?;
    Ok(())
}

// The rows of `server list`: the entries that run, then the entries of an
// untrusted project file, with their state. Nothing else happens for them
// (design rule 8).
fn list_rows(loaded: &Loaded) -> Vec<render::Row> {
    let place = |name: &str| match loaded.project_of(name) {
        Some(project) => (project.file.clone(), Some(Trust::Trusted)),
        None => (loaded.path.clone(), None),
    };
    let servers = loaded.config.servers().map(|server| {
        let (file, trust) = place(server.name.as_str());
        render::Row {
            name: server.name.to_string(),
            target: Some((render::transport_name(server), render::target(server))),
            error: None,
            file,
            trust,
            shown: None,
        }
    });
    let invalid = loaded.config.invalid().map(|(name, error)| {
        let (file, trust) = place(name);
        render::Row {
            name: name.to_string(),
            target: None,
            error: Some(render::Reason::of(error)),
            file,
            trust,
            shown: None,
        }
    });
    let waiting = loaded
        .project
        .project()
        .filter(|project| project.trust != Trust::Trusted)
        .map(render::project_rows)
        .unwrap_or_default();
    servers.chain(invalid).chain(waiting).collect()
}

// A joined word gives the fix.
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

// Always closes the connection. A server can let a connect with no token
// in and answer `401` to a request. Then `work` runs once more on a
// connection with the stored login. This is safe, because the server did
// not do the first request.
async fn with_connection<T>(
    context: &Context<'_>,
    server: &Server,
    work: impl AsyncFn(&Connection) -> Result<T, Failure>,
) -> Result<T, Failure> {
    let connection = session::open(context, server).await?;
    let result = asked(server, &connection, work(&connection).await);
    let authorized = connection.is_authorized();
    connection.close().await;
    let challenge = result
        .as_ref()
        .err()
        .and_then(Failure::login_challenge)
        .filter(|_| !authorized)
        .map(str::to_string);
    match challenge {
        Some(challenge) => {
            let connection = session::open_with_login(context, server, Some(&challenge)).await?;
            let result = asked(server, &connection, work(&connection).await);
            connection.close().await;
            result
        }
        None => result,
    }
}

// Exit 5 when the server asked a question that `ncp` refused (design
// B63): the result is not the answer to the request of the user.
fn asked<T>(
    server: &Server,
    connection: &Connection,
    result: Result<T, Failure>,
) -> Result<T, Failure> {
    if connection.refused_a_question() {
        return Err(Failure::new(
            Exit::InputRequired,
            "the server asked a question, and ncp cannot ask with no terminal",
        )
        .server(server.name.as_str())
        .hint("run the command in a terminal to answer the question"));
    }
    result
}

async fn server_show(context: &Context<'_>, word: &str) -> Result<(), Failure> {
    let loaded = session::load(context, Some(word)).await?;
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
            let access = match server.transport {
                crate::config::Transport::Stdio(_) => list_cache::Access::Stdio,
                crate::config::Transport::Http(_) | crate::config::Transport::Sse(_) => {
                    list_cache::Access::Http {
                        credentials: connection.is_authorized() || !server.value_names().is_empty(),
                    }
                }
            };
            let keep = list_cache::keep_for(hints.ttl_ms, hints.private, access);
            keep_tools(context, server, &tools, keep, has_instructions(connection));
            Ok(tools)
        }
        Err(error) => {
            super::auth::remember_scopes(context, server, &error).await;
            Err(session::request_failure(server, "tools/list", &error))
        }
    }
}

fn has_instructions(connection: &Connection) -> bool {
    connection
        .server_info()
        .is_some_and(|info| info.instructions.is_some())
}

// Keeps the tools in the cache for the time of `list_cache::keep_for`. A list
// that `ncp` must not keep removes the old file. When `ncp` cannot write the
// cache, it gives a warning: `ncp` works the same with no cache.
fn keep_tools(
    context: &Context<'_>,
    server: &Server,
    tools: &[Tool],
    ttl_ms: Option<u64>,
    has_instructions: bool,
) {
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
                has_instructions,
                tools: tools.to_vec(),
            };
            if let Err(error) = list_cache::save(&path, &cached) {
                context.ui.warning(
                    None,
                    &format!(
                        "cannot write the list cache {}: {error}",
                        path.display(),
                        error = chain_text(&error)
                    ),
                );
            }
        }
        None => list_cache::remove(&path),
    }
}

// The tools of the cache when they are fresh, so that the command needs
// no server start.
fn cached_tools(context: &Context<'_>, server: &Server) -> Option<Vec<Tool>> {
    cached_list(context, server).map(|cached| cached.tools)
}

fn cached_list(context: &Context<'_>, server: &Server) -> Option<list_cache::CachedTools> {
    if !context.cache.reads() {
        return None;
    }
    let cached = list_cache::load(&list_cache::path(&context.paths.cache_dir, &server.name))?;
    let now = list_cache::now_ms();
    let fresh = cached
        .fresh_for(&server.target, list_cache::entry_digest(server), now)?
        .clone();
    context.ui.debug(
        Some(server.name.as_str()),
        &format!(
            "tools from the list cache, {} s old",
            fresh.age(now).as_secs()
        ),
    );
    Some(fresh)
}

async fn tool_list(context: &Context<'_>, word: &str) -> Result<(), Failure> {
    let loaded = session::load(context, Some(word)).await?;
    let server = select(context, &loaded, word, show_fix)?;
    let cached = cached_list(context, server).map(|cached| (cached.tools, cached.has_instructions));
    let (tools, has_instructions) = match cached {
        Some(cached) => cached,
        None => {
            with_connection(context, server, async |connection| {
                let tools = list(context, server, connection).await?;
                Ok((tools, has_instructions(connection)))
            })
            .await?
        }
    };
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

// The tool word, or the failure that names the fix.
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
    let loaded = session::load(context, Some(word)).await?;
    let server = select(context, &loaded, word, show_fix)?;
    let name = server.name.as_str();
    let tool_name = tool_word(name, tool)?;
    // A cached list that misses the tool gets a live list: the server can
    // change within the time of the list.
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
        tool_args::parse(&call.arguments).map_err(|error| Failure::usage(chain_text(&error)))?
    };
    let loaded = session::load(context, Some(word)).await?;
    let server = session::select(&loaded, word, context.ui)?;
    let input = match input {
        Input::Stdin => Resolved::Object(read_stdin_object(context).await?),
        Input::Pairs(pairs) => Resolved::Pairs(pairs),
        Input::Object(object) => Resolved::Object(object),
    };
    let needs_tool = call.help || !call.no_validate || tool_args::needs_schema(&input);
    let name = server.name.as_str();
    with_connection(context, server, async |connection| {
        // rmcp mirrors parameters only for a tool that it listed in this
        // session, so that case lists again. In other cases, a fresh cache
        // serves.
        let cached = if needs_tool && !mirrors_parameters(server, connection) {
            cached_tools(context, server)
        } else {
            None
        };
        let prepared = match cached {
            Some(tools) => {
                // The server can change within the time of the list, and
                // the spec permits a new fetch. Thus a cached list that does
                // not fit gets one live list.
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
        let listed = tool.is_some();
        let response =
            super::ask::with_rounds(context, name, async |continuation| {
                match connection
                    .call_tool(tool_name, arguments.clone(), continuation, context.limits())
                    .await
                {
                    Ok(response) => Ok(response),
                    Err(error) => {
                        super::auth::remember_scopes(context, server, &error).await;
                        Err(call_failure(server, tool_name, listed, &error))
                    }
                }
            })
            .await?;
        print_response(context, name, tool_name, response)
    })
    .await
}

// The tool and the arguments of a call: the tool from the list, the
// arguments with the schema rule, and the schema check unless
// `--no-validate`.
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

// From `2026-07-28`, an HTTP client copies the parameters with
// `x-mcp-header` into `Mcp-Param-*` headers (SEP-2243). rmcp knows these
// parameters only from a listed tool, so the call needs the tool list
// first. A tool that the list does not show stays callable.
fn mirrors_parameters(server: &Server, connection: &Connection) -> bool {
    matches!(server.transport, crate::config::Transport::Http(_))
        && connection.protocol_version().uses_discover()
}

fn offers(
    connection: &Connection,
    feature: impl Fn(&rmcp::model::ServerCapabilities) -> bool,
) -> bool {
    connection
        .server_info()
        .is_some_and(|info| feature(&info.capabilities))
}

// A list request that a server can lack: "method not found" gives an
// empty list, because the list is a part of a larger answer.
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
    let loaded = session::load(context, Some(word)).await?;
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

// For a request that names a thing by name or URI, "invalid params" or
// "resource not found" is a usage error. The hint is the command that
// lists the names.
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
    let loaded = session::load(context, Some(word)).await?;
    let server = select(context, &loaded, word, show_fix)?;
    let name = server.name.as_str();
    let answer = with_connection(context, server, async |connection| {
        super::ask::with_rounds(context, name, async |continuation| {
            connection
                .read_resource(uri, continuation, context.limits())
                .await
                .map_err(|error| named_failure(server, "resources/read", &error, "resource"))
        })
        .await
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

// The one content of a resource as bytes: the text, or the decoded blob.
// Several contents have no raw form, because the bytes alone lose their
// borders and types.
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
            "the server sent a blob that is not base64: {error}",
            error = chain_text(&error)
        ))
    })
}

// An `input_required` result: on stdout, with exit 5, as for a tool call.
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
    let loaded = session::load(context, Some(word)).await?;
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

// The arguments of a prompt: text only, as the spec gives them.
fn prompt_arguments(words: &[String]) -> Result<JsonObject, Failure> {
    match tool_args::parse(words).map_err(|error| Failure::usage(chain_text(&error)))? {
        Input::Pairs(pairs) => Ok(pairs
            .into_iter()
            .map(|(key, text)| (key, Value::String(text)))
            .collect()),
        _ => Err(Failure::usage("prompt arguments are key=value pairs")),
    }
}

async fn prompt_get(
    context: &Context<'_>,
    word: &str,
    prompt: &str,
    words: &[String],
) -> Result<(), Failure> {
    let arguments = prompt_arguments(words)?;
    let loaded = session::load(context, Some(word)).await?;
    let server = select(context, &loaded, word, show_fix)?;
    let name = server.name.as_str();
    let answer = with_connection(context, server, async |connection| {
        super::ask::with_rounds(context, name, async |continuation| {
            connection
                .get_prompt(prompt, arguments.clone(), continuation, context.limits())
                .await
                .map_err(|error| named_failure(server, "prompts/get", &error, "prompt"))
        })
        .await
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

// The spec says that a server answers an unknown tool or bad arguments with
// "invalid params" (MCP 2025-11-25, server/tools, "Error Handling"). That is a
// usage error. When `ncp` knows the tool from the list, `tool show` gives the
// fix. In other cases the tool can be unknown, and `tool list` gives the fix.
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
            Failure::usage(format!("cannot read the arguments from stdin: {error}", error = chain_text(&error)))
        })?,
        () = context.cancel.cancelled() => return Err(session::interrupted(None)),
    };
    tool_args::object(&text).map_err(|error| Failure::usage(chain_text(&error)))
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
            // The top-level property of each problem, so that the example
            // shows it too.
            // The path is a JSON pointer (RFC 6901 §4): `~1` is `/` and
            // `~0` is `~`.
            let mut failed: Vec<String> = problems
                .iter()
                .filter_map(|problem| problem.path.split('/').nth(1))
                .filter(|name| !name.is_empty())
                .map(|name| name.replace("~1", "/").replace("~0", "~"))
                .collect();
            failed.sort_unstable();
            failed.dedup();
            let failed: Vec<&str> = failed.iter().map(String::as_str).collect();
            let example = render::example_call_with(server, tool, &failed);
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
            .hint(example))
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
