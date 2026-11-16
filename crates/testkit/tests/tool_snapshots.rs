//! Transcripts of the tool commands.

#![allow(clippy::unwrap_used)]

mod common;

use common::{Home, ncp_snapshot};
use serde_json::json;

const FIXTURE: &str = env!("CARGO_BIN_EXE_ncp-fixture");

fn home(name: &str) -> Home {
    let home = Home::new(name);
    home.configure(&json!({"fx": {"command": FIXTURE, "args": ["modern"]}}));
    home
}

#[test]
fn tool_list_shows_the_server_tools() {
    let home = home("tool-list-success");
    ncp_snapshot!(home.filters(), home.ncp(["tool", "list", "fx"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    echo           Return the text
    fail           Return a tool error
    ask            Return input_required
    summary        Return a summary, an embedded resource and structured content
    progress       Report three steps of progress
    elicit         Ask for a name and an age, as a multi-round request
    elicit-url     Ask to open the URL of the text (or ask for sampling)
    needs-page     Fail with the error that asks to open a page
    elicit-choice  Ask for a choice whose title has control characters
    elicit-legacy  Ask for a name with an elicitation request
    env            Return an environment variable
    env-names      Return the names of the environment variables, one on each line
    header         Return an HTTP request header
    wait           Wait; write the marker file when cancelled

    ----- stderr -----
    hint: The server `fx` has instructions for clients. To see the instructions, run `ncp server show fx`.
    ");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "tool", "list", "fx"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"server":"fx","tools":[{"description":"Return the text","inputSchema":{"properties":{"text":{"type":"string"}},"type":"object"},"name":"echo"},{"description":"Return a tool error","inputSchema":{"properties":{"text":{"type":"string"}},"type":"object"},"name":"fail"},{"description":"Return input_required","inputSchema":{"properties":{"text":{"type":"string"}},"type":"object"},"name":"ask"},{"description":"Return a summary, an embedded resource and structured content","inputSchema":{"type":"object"},"name":"summary"},{"description":"Report three steps of progress","inputSchema":{"properties":{"text":{"type":"string"}},"type":"object"},"name":"progress"},{"description":"Ask for a name and an age, as a multi-round request","inputSchema":{"properties":{"text":{"type":"string"}},"type":"object"},"name":"elicit"},{"description":"Ask to open the URL of the text (or ask for sampling)","inputSchema":{"properties":{"text":{"type":"string"}},"type":"object"},"name":"elicit-url"},{"description":"Fail with the error that asks to open a page","inputSchema":{"properties":{"text":{"type":"string"}},"type":"object"},"name":"needs-page"},{"description":"Ask for a choice whose title has control characters","inputSchema":{"properties":{"text":{"type":"string"}},"type":"object"},"name":"elicit-choice"},{"description":"Ask for a name with an elicitation request","inputSchema":{"properties":{"text":{"type":"string"}},"type":"object"},"name":"elicit-legacy"},{"description":"Return an environment variable","inputSchema":{"properties":{"name":{"type":"string"}},"type":"object"},"name":"env"},{"description":"Return the names of the environment variables, one on each line","inputSchema":{"type":"object"},"name":"env-names"},{"description":"Return an HTTP request header","inputSchema":{"properties":{"name":{"type":"string"}},"type":"object"},"name":"header"},{"description":"Wait; write the marker file when cancelled","inputSchema":{"properties":{"marker":{"type":"string"},"ms":{"type":"integer"},"started":{"type":"string"}},"type":"object"},"name":"wait"}]}

    ----- stderr -----
    {"hint":{"command":"ncp server show fx","message":"The server `fx` has instructions for clients. To see the instructions, run `ncp server show fx`.","server":"fx"}}
    "#
    );
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
}

#[test]
fn tool_list_for_an_unknown_server_suggests_a_close_name() {
    let home = home("tool-list-unknown");
    ncp_snapshot!(home.filters(), home.ncp(["tool", "list", "fz"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No server `fz` is configured in `[HOME]/config/ncp/mcp.json`

    hint: Did you mean `fx`?

    hint: To see the configured servers, run `ncp server list`
    ");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "tool", "list", "fz"]),
        @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    {"error":{"causes":[],"code":"usage","data":[],"exit":2,"hints":[{"message":"Did you mean `fx`?"},{"command":"ncp server list","message":"To see the configured servers, run `ncp server list`"}],"message":"No server `fz` is configured in `[HOME]/config/ncp/mcp.json`"}}
    "#
    );
    run.error();
}

#[test]
fn tool_show_describes_one_tool() {
    let home = home("tool-show-success");
    ncp_snapshot!(home.filters(), home.ncp(["tool", "show", "fx", "echo"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    Tool: echo

    Return the text

    Arguments:
      text (string, optional)

    Output:
      not described

    Annotations (hints from the server, not checked):
      read-only: no (default)
      destructive: yes (default)
      idempotent: no (default)
      open-world: yes (default)

    Example:
      ncp tool call fx echo
    ");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "tool", "show", "fx", "echo"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"server":"fx","tool":{"description":"Return the text","inputSchema":{"properties":{"text":{"type":"string"}},"type":"object"},"name":"echo"}}
    "#
    );
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
}

#[test]
fn tool_show_for_an_unknown_tool_fails() {
    let home = home("tool-show-unknown");
    ncp_snapshot!(home.filters(), home.ncp(["tool", "show", "fx", "nope"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The server `fx` has no tool `nope`

    hint: To see the tools of the server, run `ncp tool list fx`
    ");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "tool", "show", "fx", "nope"]),
        @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    {"error":{"causes":[],"code":"usage","data":[],"exit":2,"hints":[{"command":"ncp tool list fx","message":"To see the tools of the server, run `ncp tool list fx`"}],"message":"The server `fx` has no tool `nope`","server":"fx"}}
    "#
    );
    run.error();
}

#[test]
fn tool_call_prints_the_tool_result() {
    let home = home("tool-call-success");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["tool", "call", "fx", "echo", "text=hi"]),
        @"
    exit_code: 0 (success)
    ----- stdout -----
    hi
    "
    );
    assert_eq!(run.0.stdout, b"hi\n");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "tool", "call", "fx", "echo", "text=hi"]),
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"content":[{"text":"hi","type":"text"}],"isError":false,"resultType":"complete"}
    "#
    );
    assert!(run.0.stdout.ends_with(b"\n"));
    assert_eq!(run.warnings(), [] as [serde_json::Value; 0]);
}

#[test]
fn tool_call_for_an_unknown_tool_fails() {
    let home = home("tool-call-unknown");
    ncp_snapshot!(home.filters(), home.ncp(["tool", "call", "fx", "nope"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The server `fx` has no tool `nope`

    hint: To see the tools of the server, run `ncp tool list fx`
    ");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "tool", "call", "fx", "nope"]),
        @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    {"error":{"causes":[],"code":"usage","data":[],"exit":2,"hints":[{"command":"ncp tool list fx","message":"To see the tools of the server, run `ncp tool list fx`"}],"message":"The server `fx` has no tool `nope`","server":"fx"}}
    "#
    );
    run.error();
}

#[test]
fn tool_call_with_a_tool_error_result_exits_with_one() {
    let home = home("tool-call-tool-error");
    let run = ncp_snapshot!(home.filters(), home.ncp(["tool", "call", "fx", "fail", "text=bad"]), @"
    exit_code: 1 (failure)
    ----- stdout -----
    bad

    ----- stderr -----
    error: The tool `fail` of the server `fx` returned an error
    ");
    assert_eq!(run.0.stdout, b"bad\n");
    let run = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "tool", "call", "fx", "fail", "text=bad"]),
        @r#"
    exit_code: 1 (failure)
    ----- stdout -----
    {"content":[{"text":"bad","type":"text"}],"isError":true,"resultType":"complete"}

    ----- stderr -----
    {"error":{"causes":[],"code":"tool","data":[],"exit":1,"hints":[],"message":"The tool `fail` of the server `fx` returned an error","server":"fx"}}
    "#
    );
    run.error();
}

#[test]
fn tool_call_asks_the_form_of_the_server_on_a_tty() {
    let home = home("tool-call-form").set("NCP_TEST_TTY", "1");
    let text = ncp_snapshot!(
        home.filters(),
        home.ncp(["tool", "call", "fx", "elicit", "text=x"]),
        input = "\nAda\ny\n",
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"action":"accept","content":{"age":30,"name":"Ada"}}

    ----- stderr -----
    The server fx asks: Who are you?
      age (whole number) [30]:   name (required, text): Send these values to fx? [s]end, c[h]ange a field, [d]ecline, [c]ancel:
    "#
    );
    assert_eq!(text.code(), 0, "{}", text.describe());
    let json = ncp_snapshot!(
        home.filters(),
        home.ncp(["--output-format", "json", "tool", "call", "fx", "elicit", "text=x"]),
        input = "\nAda\ny\n",
        @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"content":[{"text":"{\"action\":\"accept\",\"content\":{\"age\":30,\"name\":\"Ada\"}}","type":"text"}],"isError":false,"resultType":"complete"}

    ----- stderr -----
    The server fx asks: Who are you?
      age (whole number) [30]:   name (required, text): Send these values to fx? [s]end, c[h]ange a field, [d]ecline, [c]ancel:
    "#
    );
    let answer: serde_json::Value = serde_json::from_slice(&json.0.stdout).unwrap();
    assert_eq!(answer["content"][0]["text"], text.stdout_text().trim_end());
    // The snapshot drops the trailing space of the question, so check it here.
    for run in [&text, &json] {
        assert!(run.0.stderr.ends_with(b"[d]ecline, [c]ancel: "));
    }
}
