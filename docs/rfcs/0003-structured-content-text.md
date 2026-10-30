---
state: accepted
supersedes: []
---

# 0003: `structuredContent`, embedded resources and JSON layout in the text output

## Summary

When a tool result has `structuredContent`, the text output of `tool call`
shows it after the content blocks. ncp takes the data out of a
`{"result": ...}` wrapper. ncp does not show a text block that the copy
check finds (see "The copy check"). Thus the model usually reads the data
one time.

The text output of `tool call` and `prompt get` also shows the text of an
embedded resource.

A JSON part of the text output gets the layout and the escapes of the JSON
output: indented on a TTY, and one line in a pipe. This includes the
`input_required` result of `tool call`, `resource read` and `prompt get`.

The JSON output does not change.

## Background

### The two fields of a tool result

A tool result (`CallToolResult`) holds its data in two fields
([2026-07-28 schema, `CallToolResult`][schema]):

- `content` is required. It is a list of content blocks, and the list can
  be empty. Each block has a type: `text`, `image`, `audio`,
  `resource_link` or `resource`. A `resource` block is an embedded
  resource. It holds the contents of a resource as `text` or as `blob`.
- `structuredContent` is optional. It holds the result data of the tool as
  JSON: an object, an array, a string, a number, a boolean or `null`. It is
  not a structured output of the model.

Two rules of the spec connect the fields:

- When a tool has an `outputSchema`, the server MUST give
  `structuredContent` that obeys the schema
  ([2026-07-28, Tools, Output Schema][output]).
- "For backwards compatibility, a tool that returns structured content
  SHOULD also return the serialized JSON in a TextContent block"
  ([2026-07-28, Tools, Structured Content][tools]).

Thus a text block is usually a copy of `structuredContent`. But the spec
does not require a copy. The `list_users` example of the spec has a text
block with a summary, and the user ids are only in `structuredContent`.

### How servers fill the two fields

The server, not ncp, selects the form of the text blocks. Most servers use
the server part of an MCP SDK. ncp uses only the client part of rmcp.

| Server SDK | Text blocks | `structuredContent` |
|---|---|---|
| [rmcp 3.5.1][rmcp] (Rust), `CallToolResult::structured` | One block: `structuredContent` as one-line JSON | The return data |
| [Official Python SDK (`mcp`) 1.27.2][python], return type that is an object | One block: indented JSON | The return data |
| Official Python SDK, return type that is not an object | One block: the string, or indented JSON. A list gives [one block for each element][python-list] | `{"result": ...}` with the return data |
| [fastmcp 3.3.1][fastmcp] (Python), return type that is not an object | One block: the string, or one-line JSON | `{"result": ...}` with the return data |
| Official TypeScript SDK 1.32.1 | No block from the SDK. The server author writes the text blocks, with any text | The server author writes it. The SDK only validates it with the output schema ([`validateToolOutput`][typescript]) |
| No SDK: the `list_users` example of the spec | A summary | The data |

In the rmcp and Python SDKs, the SDK makes both fields from the return data
of the tool function. The tool author does not select the form.

### Current output of ncp

The model of an agent reads the text output of `tool call`. Today, the
text output shows each content block:

- A text block shows its text.
- An `image` or `audio` block shows a placeholder line, for example
  `[image image/png, 4 bytes as base64]`.
- A `resource_link` block shows the line `[resource link URI]`.
- A `resource` block shows only the line `[resource URI]`.

The text output shows `structuredContent` only when `content` is empty.
Then ncp writes it as indented JSON, also in a pipe.

`tool call`, `resource read` and `prompt get` can get an `input_required`
result. The text output writes this result as indented JSON, also in a
pipe.

The JSON output shows both fields of a tool result. Of the JSON that ncp
writes, only the JSON output changes its layout with the TTY: indented on a
TTY, and one line in a pipe.

## Problem

The model loses data in two cases:

- A text block is a summary, and the data is only in `structuredContent`.
  In the `list_users` example of the spec, the model loses the user ids
  (Example 1). The model needs an id for the next call.
- A `resource` block shows only its URI. The model loses the text of the
  resource (Example 2).

The text output also has two faults in its JSON:

- It writes indented JSON in a pipe, where the model reads it. The indent
  adds tokens and no data.
- It writes DEL, a C1 control character or an invisible character in the
  JSON as a Rust escape, for example `\u{200b}`. Then the JSON does not
  parse.

## Proposal

### The payload

The payload is the data that ncp shows from `structuredContent`:

- If `structuredContent` is an object with only the key `result`, the
  payload is the data in `result`. This step is the unwrap.
- If not, the payload is all of `structuredContent`.

### JSON parts of the text output

A JSON part is a JSON value that the text output writes on its own lines.
The text output has two JSON parts:

- The payload, when it is not a string.
- The `input_required` result of `tool call`, `resource read` and
  `prompt get`.

"TTY" and "pipe" refer to stdout. ncp writes a JSON part as the JSON output
writes JSON:

- On a TTY, the JSON is indented.
- In a pipe, the JSON is on one line.
- These characters are JSON escapes, for example `\u200b`:
  - DEL and the C1 control characters.
  - A character that changes the order of text, has no width, or looks like
    a space.

serde_json already writes the C0 control characters as JSON escapes. Thus
each JSON part is valid JSON. A text block is not a JSON part, also when
its text is JSON. A JSON value in a field line of `tool show`, for example
`Default: 5`, is not a JSON part. It stays on one line.

### Text output of `tool call`

When the result has `structuredContent`, ncp writes these parts, in this
order:

1. The content blocks, in the order of the server. ncp does not show a copy
   that the copy check finds.
2. The payload. If the payload is a string, ncp writes it as it writes a
   text block. If not, ncp writes the payload as a JSON part (see "JSON
   parts of the text output").

When a tool result (`CallToolResult`) has no `structuredContent`, the
output does not change. The rule for embedded resources is the only
exception.

### The copy check

A text block is a content block of the type `text`. A copy is a text block,
or a list of text blocks, with the same data as `structuredContent`. The
copy check finds a copy by these rules.

A text matches JSON data in two cases:

- The text, parsed as JSON, is equal to the data.
- The data is a JSON string, and the text is equal to that string.

Equal means equal in serde_json. Thus the integer `5` and the float `5.0`
are not equal.

The copy check finds a text block when its text matches
`structuredContent` or the payload.

The copy check also finds all text blocks when both of these are true:

- The payload is an array with one element for each text block.
- Each text block matches the element at the same position.

### Embedded resources

- A `resource` block with `text` shows the line `[resource URI]`. Then it
  shows the text, as ncp writes a text block.
- A `resource` block with `blob` shows only the line `[resource URI]`, as
  today.

This rule applies to `tool call` and to `prompt get`, because the two
commands show a content block in the same way. It applies also to a result
with no `structuredContent`. In `prompt get`, the role comes first:

```text
user: [resource file:///notes.txt]
The text of the notes.
```

### Changes to the output

- A result with `structuredContent` and content blocks shows the payload
  after the content blocks. The copies that the copy check finds do not
  show.
- A result with `structuredContent` and no content blocks shows the
  payload, not all of `structuredContent`. In a pipe, JSON is on one line.
  Before this change, ncp indented it.
- A string payload shows as text, with no quotes and no JSON escapes.
  Before this change, ncp wrote it as JSON.
- A `resource` block with `text` shows its text.
- An `input_required` result is on one line in a pipe. Before this change,
  ncp indented it.
- A JSON part has JSON escapes (see "JSON parts of the text output").
  Before this change, it had Rust escapes.

These do not change:

- The JSON output.
- The exit codes, and the stderr lines of an error result and of an
  `input_required` result.
- The lines of `image`, `audio` and `resource_link` blocks.
- The `_meta` fields and the annotations. They do not show.
- The layout of JSON on a TTY. It stays indented.

## Examples

Each example gives a tool result, and then the text output in a pipe before
and after this change. Example 1 also shows the output on a TTY. Examples
3, 4 and 5 come from servers on the official Python SDK.

### Example 1: summary text

The `list_users` example of the spec
([2026-07-28, Tools, Structured Content][tools]).

```json
{
  "content": [
    {
      "type": "text",
      "text": "Found 2 users: Alice (alice@example.com) and Bob (bob@example.com)."
    }
  ],
  "structuredContent": [
    {"id": "1", "name": "Alice", "email": "alice@example.com"},
    {"id": "2", "name": "Bob", "email": "bob@example.com"}
  ]
}
```

Before:

```text
Found 2 users: Alice (alice@example.com) and Bob (bob@example.com).
```

After:

```text
Found 2 users: Alice (alice@example.com) and Bob (bob@example.com).
[{"email":"alice@example.com","id":"1","name":"Alice"},{"email":"bob@example.com","id":"2","name":"Bob"}]
```

After, on a TTY:

```text
Found 2 users: Alice (alice@example.com) and Bob (bob@example.com).
[
  {
    "email": "alice@example.com",
    "id": "1",
    "name": "Alice"
  },
  {
    "email": "bob@example.com",
    "id": "2",
    "name": "Bob"
  }
]
```

The summary is not a copy, so it stays. The ids show after the summary.

### Example 2: embedded resource

The tool `get-resource-reference` of the everything server 2026.8.31
([source][everything]). The result has no `structuredContent`.

```json
{
  "content": [
    {"type": "text", "text": "Returning resource reference for Resource 1:"},
    {
      "type": "resource",
      "resource": {
        "uri": "demo://resource/dynamic/text/1",
        "mimeType": "text/plain",
        "text": "Resource 1: This is a plaintext resource created at 10:36:22 PM"
      }
    },
    {
      "type": "text",
      "text": "You can access this resource using the URI: demo://resource/dynamic/text/1"
    }
  ]
}
```

Before:

```text
Returning resource reference for Resource 1:
[resource demo://resource/dynamic/text/1]
You can access this resource using the URI: demo://resource/dynamic/text/1
```

After:

```text
Returning resource reference for Resource 1:
[resource demo://resource/dynamic/text/1]
Resource 1: This is a plaintext resource created at 10:36:22 PM
You can access this resource using the URI: demo://resource/dynamic/text/1
```

The text of the resource shows after its URI line. The model gets the text
with no second call.

### Example 3: indented copy of an object

A tool with the return type `dict[str, Any]`. The text block holds
`structuredContent` as indented JSON.

```json
{
  "content": [
    {"type": "text", "text": "{\n  \"id\": 7,\n  \"state\": \"open\"\n}"}
  ],
  "structuredContent": {"id": 7, "state": "open"}
}
```

Before:

```text
{
  "id": 7,
  "state": "open"
}
```

After:

```text
{"id":7,"state":"open"}
```

The copy check finds the copy, so it does not show. The payload shows on one
line, with fewer tokens. A server on rmcp writes the copy on one line. For
this result, the output after this change is the same.

### Example 4: string result

A tool with the return type `str`.

```json
{
  "content": [{"type": "text", "text": "# Report\nAll good."}],
  "structuredContent": {"result": "# Report\nAll good."}
}
```

Before and after:

```text
# Report
All good.
```

The unwrap gives the string. The text block is equal to the string, so the
copy check finds it. The string shows one time, with its line breaks.

### Example 5: list result

A tool with the return type `list[str]`. The SDK writes one text block for
each element.

```json
{
  "content": [
    {"type": "text", "text": "a"},
    {"type": "text", "text": "b"}
  ],
  "structuredContent": {"result": ["a", "b"]}
}
```

Before:

```text
a
b
```

After:

```text
["a","b"]
```

Each text block matches the element at the same position, so the copy
check finds all text blocks. The JSON shows where each element starts and stops.

### Example 6: error result

```json
{
  "content": [{"type": "text", "text": "Rate limit. Retry in 30 s"}],
  "structuredContent": {"code": 429},
  "isError": true
}
```

Before:

```text
Rate limit. Retry in 30 s
```

After:

```text
Rate limit. Retry in 30 s
{"code":429}
```

The text is not a copy, so it stays. It tells the model when to try again.
The stderr line and the exit code 1 do not change.

### Example 7: table text

This example shows a cost.

```json
{
  "content": [
    {"type": "text", "text": "| id | state |\n|---|---|\n| 7 | open |"}
  ],
  "structuredContent": {"id": 7, "state": "open"}
}
```

Before:

```text
| id | state |
|---|---|
| 7 | open |
```

After:

```text
| id | state |
|---|---|
| 7 | open |
{"id":7,"state":"open"}
```

The table has the data of `structuredContent`, but the table is not JSON.
The copy check does not find the copy, so the data shows two times. The
cost is tokens. No data is lost.

## Reasons

### Show `structuredContent`

`structuredContent` can hold data that the text blocks do not have, as in
Example 1. The model needs this data.

### Do not show a copy

By the SHOULD rule of the spec, a text block is usually a copy of
`structuredContent`. If ncp shows both, the model reads the same data two
times. The copy check finds the usual forms of copy that the SDKs write.
"Costs" lists the forms that it does not find. This table gives the forms
that it finds:

| Form of the copy | Part of the copy check | Example |
|---|---|---|
| rmcp: `structuredContent` as one-line JSON | Matches `structuredContent` | None |
| Python, object: indented JSON | Matches `structuredContent` | 3 |
| Python and fastmcp, not an object: the string, or the JSON of the data in `result` | Matches the payload | 4 |
| Python, list: one block for each element | Matches element by element | 5 |

A text block that is not a copy stays, because it can hold data that
`structuredContent` does not have. Examples are a summary, an error
message, or a list in the text.

### Show the payload, not the copy

As JSON, the payload is one line in a pipe. An indented copy uses more
tokens. A string payload shows as text, with its line breaks.

### Use the JSON layout of the JSON output

A program or the model of an agent reads a pipe. There, one line uses fewer
tokens, and the indent adds no data. A person reads a TTY, and indented JSON
is easier to read. The JSON output uses this layout for the same reasons.

One rule for the JSON parts and the JSON output is simpler than one rule
for each output format. If `input_required` stays indented, one command
writes two JSON layouts to a pipe. `tool call` writes the payload, or an
`input_required` result.

The text output keeps one format on a TTY and in a pipe. Only the white
space of a JSON part changes. `docs/command-development.md` gives one
exception: `resource read`. With the escapes of the JSON output, a program
can parse each JSON part.

### Unwrap the payload

The Python SDKs put return data that is not an object into
`{"result": ...}`. With no unwrap, this object never matches the text, and
the data shows two times. Example 4 with no unwrap shows this output:

```text
# Report
All good.
{"result":"# Report\nAll good."}
```

### No rule for errors

Clients SHOULD give tool execution errors to the model ([2026-07-28,
Tools, Error Handling][errors]). The text of an error is usually not a
copy, so it shows (Example 6). When it is a copy, the payload shows the
same data. Thus the design needs no rule for errors.

### A fault of the copy check costs tokens, not data

The copy check finds a copy only when the text blocks have the same data
as `structuredContent`. If the check does not find a copy, the data shows
two times (Example 7). Thus a fault of the check costs tokens, and no data
is lost. "Costs" gives two exceptions: a number that serde_json cannot hold
exactly, and a JSON object with a duplicate key.

When data does not show, the model can call the tool again. A second call
can repeat a side effect, for example a second issue. This design gives the
model no reason to call a tool again.

## Alternatives

- **Text blocks only (the current output).** The data that only
  `structuredContent` has does not show, as in Example 1.
- **`structuredContent` only, with no text blocks.** Codex does this
  ([`as_function_call_output_payload`][codex]). A text block with data
  that `structuredContent` does not have does not show, for example an
  error message.
- **The payload and a placeholder line that counts the text blocks that do
  not show.** Data does not show, and the line can make a model call the
  tool again.
- **Text blocks, and then `structuredContent` if no text block parses to
  it.** This has no unwrap. The data of each Python result that is not an
  object shows two times, because `{"result": ...}` never matches the text.
- **A new JSON shape, for example `structuredContent` alone.** The type
  changes from tool to tool, so a script must find the type first. Models
  know the MCP shape, and `jq` gives the inner fields.
- **Indented JSON parts, also in a pipe (the current layout).**
  The indent adds tokens for the model.
- **One-line JSON parts, also on a TTY.** A person reads a
  long object on one line.
- **Compact JSON in a text block, or leave out nulls.** This changes the
  text that the server wrote, for example the contents of a JSON file.

## Costs

Some copies show two times, because the copy check does not find them:

- Text with the data in a form that is not JSON, for example a table
  (Example 7) or YAML.
- A number with a different JSON number type. For example, a Python
  `-> float` tool that returns `5` gives the text `5` and
  `{"result":5.0}`.
- A list of lists from the official Python SDK. The SDK makes one text
  block for each inner element.

Some data shows in a different form:

- A list of strings with line breaks shows as JSON, with the line breaks
  as escapes.
- A tool whose own `structuredContent` is `{"result": ...}` loses the key
  name.
- A string payload and a number payload can show the same, for example
  `"5"` and `5`.
- ncp sorts the keys of the JSON objects in the payload.
- A tool of the official Python SDK whose return type is a content block,
  for example `-> ImageContent`, shows its base64 data in the payload.

A large `structuredContent` next to a short summary adds many tokens.
Examples are a long list, or the UI tree of a fastmcp Prefab app.

A JSON part has a different layout on a TTY and in a pipe.
A person who pastes the output of a terminal into a script or a test gets
the indented form.

A number that serde_json cannot hold exactly shows rounded. An example is
an integer outside the 64-bit range. Its exact text block does not show,
because the text and the number are equal after the parse.

A text block with a duplicate key in a JSON object, for example
`{"a":1,"a":2}`, does not show when `structuredContent` is `{"a":2}`. The
parse of serde_json keeps the last key, so the text and `structuredContent`
are equal. The `"a":1` does not show.

## Documents to change

The commit that completes the build changes these sentences.

In `docs/command-development.md`, "Streams and formats", the sentence "In
the text output, `ncp` indents a JSON value on its own lines, also in a
pipe." becomes:

> In the text output, a JSON value on its own lines has the layout and the
> escapes of the JSON value on stdout in the JSON output.

In STYLE.md, "Plain text", the bullet about a placeholder gets this
sentence:

> A text block with the same data as `structuredContent` does not show,
> and it gets no placeholder.

In STYLE.md, "Plain text", the bullet about a JSON value and the text of a
server gets this sentence:

> `tool call` shows `structuredContent` without its `{"result": ...}`
> wrapper.

## Not in scope

- Image and audio data for the model.
- The key order of the JSON output.

[tools]: https://modelcontextprotocol.io/specification/2026-07-28/server/tools#structured-content
[output]: https://modelcontextprotocol.io/specification/2026-07-28/server/tools#output-schema
[schema]: https://github.com/modelcontextprotocol/modelcontextprotocol/blob/271ecc9accafdd9b83a3c869fa67c22953b2af80/schema/2026-07-28/schema.ts#L1809
[errors]: https://modelcontextprotocol.io/specification/2026-07-28/server/tools#error-handling
[python]: https://github.com/modelcontextprotocol/python-sdk/blob/v1.27.2/src/mcp/server/fastmcp/utilities/func_metadata.py#L126
[fastmcp]: https://github.com/PrefectHQ/fastmcp/blob/v3.3.1/fastmcp_slim/fastmcp/tools/base.py#L325
[codex]: https://github.com/openai/codex/blob/1cc7dc8f2acc63ce6c1cb72271e6b90a76136c2b/codex-rs/protocol/src/models.rs#L2467
[typescript]: https://github.com/modelcontextprotocol/typescript-sdk/blob/1.32.1/src/server/mcp.ts#L344
[rmcp]: https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.5.1/crates/rmcp/src/model.rs#L4155
[everything]: https://github.com/modelcontextprotocol/servers/blob/2026.8.31/src/everything/tools/get-resource-reference.ts
[python-list]: https://github.com/modelcontextprotocol/python-sdk/blob/v1.27.2/src/mcp/server/fastmcp/utilities/func_metadata.py#L507
