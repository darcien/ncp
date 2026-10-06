# Research: early tests of risks

Status: 2026-10-07. Baseline: [target-requirements.md](../target-requirements.md).
Plan: [plan.md](../plan.md), milestone 1. Code reading: [rmcp-open-points.md](rmcp-open-points.md),
[stack-rust.md](stack-rust.md).

Method: I wrote a scratch crate and ran it on macOS 15.7 (arm64), Rust 1.99.0.
Versions: `rmcp` 3.5.1, `reqwest` 0.13.5, `hyper-util` 0.1.21, `rustls` 0.23.45
(`ring` provider), `tokio` 1.53.2, `keyring` 3.6.3. The test code is in
`.scratch/` (not in git). All servers are local: an rmcp fixture server on
stdio and on Streamable HTTP (`axum` 0.8), and a small recording proxy. No
test calls a remote host. I did not run tests on Linux.

The fixture has three modes:

- **modern**: answers `server/discover` at once. It also accepts `initialize`.
- **legacy**: answers `server/discover` with `-32601` (method not found), as
  a `2025-11-25` server does. rmcp's own tests use the same method.
- **slow-N**: answers `server/discover` after N ms. It also accepts
  `initialize`.

## Summary

| # | Test | Result |
|---|---|---|
| 1 | rmcp client, stdio and Streamable HTTP, modern and legacy mode | Pass. All four work: `tools/list` and `tools/call`. |
| 2 | Era detection with our own timeout | Pass. Legacy and slow servers fall back on stdio and HTTP. rmcp `Auto` fails on a late answer, as the code reading said. |
| 3 | Cancellation after Ctrl-C | Pass with a stdio server in its own process group. In the same group, SIGINT stops the server before the cancel request. |
| 4 | reqwest proxy variables | Pass. Upper and lower case, `NO_PROXY`, and credentials work. A loopback target goes through the proxy when `NO_PROXY` does not name it. |
| 5 | Timings through our `reqwest::Client` | Pass for DNS, connect (DNS + TCP + proxy + TLS together) and response time. No separate TCP, `CONNECT` and TLS times. |
| 6 | `keyring` 3.6 with the zbus Secret Service store for `x86_64-unknown-linux-musl` | Pass. It builds, and it links with `rust-lld` to a static ELF. No C code. Not run. |

## 1. Transports and eras

| Transport | Lifecycle mode | Server | Result |
|---|---|---|---|
| stdio | `Discover` (`2026-07-28`) | modern | Pass. Era `2026-07-28`. |
| stdio | `Initialize` | legacy | Pass. Era `2025-11-25`. |
| Streamable HTTP | `Discover` | modern | Pass. |
| Streamable HTTP | `Initialize` | legacy | Pass. |
| stdio | `Discover` | legacy | `ClientInitializeError::JsonRpcError`. `Discover` mode does not fall back. |

## 2. Era detection with our own timeout

Method: connect in `Discover` mode inside `tokio::time::timeout`. On a
`JsonRpcError` (legacy server) or on a timeout, make a new transport and
connect in `Initialize` mode. For stdio, the new transport is a new process.
The old transport is dropped, so a late answer cannot reach the new
connection.

| Case | Probe timeout | Result |
|---|---|---|
| Legacy stdio server | 2 s | Fallback after the error. Era `2025-11-25`. |
| Legacy HTTP server | 2 s | Fallback after the error, with no wait. |
| Modern stdio server, answers after 200 ms | 2 s | Modern. No fallback. |
| Modern stdio server, answers after 1500 ms | 300 ms | Timeout, new process, `initialize`. Calls work, also after the late answer time. |
| Modern HTTP server, answers after 1500 ms | 300 ms | Timeout, new transport, `initialize`. Calls work. |
| rmcp `Auto`, stdio server answers after 11 s | 10 s (fixed) | Fails after 11.3 s: `conflict initialized response id: expected 1, got 0`. |

Results:

- Our method works. Do not use rmcp `Auto` mode. The late answer of
  `server/discover` arrives on the same transport, and rmcp stops with
  `ConflictInitResponseId`. This confirms point 1 of
  [rmcp-open-points.md](rmcp-open-points.md).
- After a timeout, a dual-era server answers `initialize` as a legacy server.
  The era that we find is then "legacy", but the server is modern. Thus the
  era cache must record why it chose legacy. A legacy result from a timeout is
  weaker than a legacy result from an error. The next run can probe again.
- A modern-only server that is slow does not accept `initialize`. Our
  fallback then fails. I did not test this case. The error message must tell
  the user about the probe timeout.

## 3. Cancellation after Ctrl-C

Method: the test acts as a terminal. It starts the client in a new process
group, and sends SIGINT to the whole group (`kill -INT -- -PGID`). The client
starts the stdio server, sends `tools/call` with
`Peer::send_cancellable_request`, and waits for Ctrl-C with
`tokio::signal::ctrl_c`. Then it calls `RequestHandle::cancel`. The server
writes a marker file when its request token is cancelled.

| Server process group | Server gets `notifications/cancelled` | Server alive after Ctrl-C |
|---|---|---|
| Own group (`Command::process_group(0)`) | Yes | Yes |
| Same group as the client | No | No (killed by SIGINT) |

Results:

- Start each stdio server in its own process group. Otherwise the terminal
  stops the server before the client can send the cancel request.
- In the same-group case, `RequestHandle::cancel` returns `Ok(())`, because
  the write to the pipe succeeds. The client cannot see that the server did
  not get the message.
- A server in its own group does not get the SIGINT of the terminal. `ncp`
  must stop it. In the test, the rmcp server stopped when its stdin closed,
  and no process stayed. A server that ignores the end of stdin can stay
  alive. The stdio shutdown of the spec (close stdin, wait, SIGTERM, SIGKILL)
  is necessary.

## 4. Proxy variables

Method: a new probe process for each case, with `env_clear()` and only the
variables of the case. The probe builds a default `reqwest::Client` and sends
one request. The proxy records the request line and the
`Proxy-Authorization` header. It sends every request to the local fixture.
Host names such as `origin.test` exist only in the proxy, so a direct
connection fails with a DNS error.

| Variables | Target | Result |
|---|---|---|
| `HTTP_PROXY` | `http://origin.test/x` | Proxy: `GET http://origin.test/x` |
| `http_proxy` | `http://origin.test/x` | Proxy |
| `HTTPS_PROXY` | `https://secure.test/x` | Proxy: `CONNECT secure.test:443` |
| `https_proxy` | `https://secure.test/x` | Proxy: `CONNECT` |
| `HTTPS_PROXY` only | `http://origin.test/x` | Direct (DNS error). `HTTPS_PROXY` is for `https` only. |
| `HTTPS_PROXY` and a different `https_proxy` | `https://secure.test/x` | Upper case wins |
| `ALL_PROXY` | `https://secure.test/x` | Proxy |
| `HTTPS_PROXY=http://user:pa%40ss@…` | `https://secure.test/x` | `CONNECT` with `Proxy-Authorization: Basic` of `user:pa@ss` (percent-decoded) |
| `HTTP_PROXY=http://user:pa%40ss@…` | `http://origin.test/x` | `GET` with the same header |
| `NO_PROXY=secure.test` | `https://secure.test/x` | Direct |
| `no_proxy=secure.test` | `https://secure.test/x` | Direct |
| `NO_PROXY=secure.test` | `https://a.secure.test/x` | Direct. A bare domain also matches its subdomains. |
| `NO_PROXY=.secure.test` | `https://a.secure.test/x` | Direct |
| `NO_PROXY=*` | `https://secure.test/x` | Direct |
| `NO_PROXY=127.0.0.1` or `127.0.0.0/8` | `http://127.0.0.1:PORT/x` | Direct. IP and CIDR forms work. |
| `HTTP_PROXY`, no `NO_PROXY` | `http://127.0.0.1:PORT/x` | **Proxy.** reqwest does not skip loopback by itself. |
| `HTTP_PROXY` and `REQUEST_METHOD` | `http://origin.test/x` | Direct. reqwest ignores `HTTP_PROXY` in a CGI environment. |
| `HTTP_PROXY`, rmcp Streamable HTTP with our client | `http://mcp.test/mcp` | Proxy. `server/discover` and `tools/list` work. |

Results:

- reqwest meets the proxy requirement for MCP traffic when `ncp` gives its
  own `reqwest::Client` to rmcp.
- A local HTTP server goes through the proxy when the user sets a proxy and
  `NO_PROXY` does not name loopback. Two options: keep the reqwest behavior
  and tell users to add `localhost,127.0.0.1` to `NO_PROXY`, or always skip
  the proxy for loopback. The command-line design decides.
- The automated tests of `ncp` must clear the proxy variables, or add
  loopback to `NO_PROXY`, because they use local servers.
- With a proxy, the client does no DNS lookup for the target. The proxy does
  it.
- I did not test TLS through the tunnel, because the fixture has no TLS.

## 5. Timings

Method: a `reqwest::Client` with a resolver wrapper (`dns_resolver`) and a
tower layer on the connector (`connector_layer`). A wrapper of rmcp's
`StreamableHttpClient` trait measures each POST. The fixture answers
`server/discover` after 150 ms. The URL uses `localhost`, so that the resolver
runs.

| Measure | Value | Source |
|---|---|---|
| DNS (`localhost`) | 2.0 ms | Resolver wrapper |
| Connect | 2.2 ms, one time | Connector layer. Includes DNS, TCP, proxy `CONNECT` and TLS. |
| Response, `server/discover` | 157.6 ms | `StreamableHttpClient` wrapper |
| Response, two `tools/list` | 1.3 ms, 1.0 ms | Same |

Results:

- DNS, connect and response time are available with no fork. The client
  keeps the connection: three POSTs used one connect.
- reqwest has no hook for TCP, proxy `CONNECT` and TLS as separate phases.
  For that split, `ncp` needs its own connector on hyper-util (see
  [stack-rust.md](stack-rust.md) section 4). This is a SHOULD item, so the
  combined connect time is enough for the MVP.
- The response time of an SSE response ends at the response headers. The
  time of the full stream needs a measure at the rmcp `Peer`.

## 6. `keyring` 3.6 for Linux musl

Method: a crate with `keyring` 3.6.3, features `async-secret-service`,
`crypto-rust`, `tokio`. Target `x86_64-unknown-linux-musl`, added with
`rustup target add`.

| Step | Result |
|---|---|
| `cargo build` (library) | Pass. 134 crates. |
| Link a binary with the default linker (`cc` on macOS) | Fail. The macOS linker cannot make an ELF file. |
| Link a binary with `-C linker=rust-lld` | Pass. Static-pie ELF, x86-64. |
| C code | None. The graph has no `cc` crate. |
| Run on Linux | Not done. No Linux host. |

Results:

- The zbus Secret Service store builds for musl with pure Rust. A release
  build for Linux musl can use `rust-lld`. The TLS provider (`ring`) needs a
  C compiler for musl; I did not test that part.
- A test on Linux with and without a Secret Service is still open (open item
  2 of the requirements).

## Effect on the plan

| Milestone | Change |
|---|---|
| 2 (domain core) | The era cache records the reason for a legacy result: an error from the server, or a probe timeout. |
| 4 (client core) | Use `Discover` in our own timeout, then a new transport with `Initialize`. Do not use `Auto`. Start stdio servers with `process_group(0)` and stop them as the spec describes. Give rmcp our own `reqwest::Client`. |
| 4 (tests) | The fixture server needs an HTTP server. `axum` worked in the scratch crate. A dev-dependency must pass `deny.toml`. |
| 5 (design) and after | Decide the proxy rule for loopback. Per-phase network timings need a hyper-util connector. |
