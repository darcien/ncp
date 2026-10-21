# Live tests

The live tests run `ncp` against the real parts of this Mac and a real
server: the login Keychain, a browser, the system settings, and a login
with the account of the maintainer. The other tests are sealed: they use only local stubs,
fixtures and temporary folders. A live test changes the state of this Mac,
so only the maintainer runs them, and the git hook only compiles them.

Run them before a release, and after a change to the store, the login or
the proxy code.

## Run

```sh
mise run live            # All tests for this Mac.
mise run live keychain   # One test. The filter is a part of the test name.
```

The task builds `ncp`, and runs the tests in
`crates/testkit/tests/live.rs`. Each test uses its own temporary XDG
folders, so the configuration of the maintainer does not change. The Keychain is shared:
the tests use only the names `NCP_LIVE_CHECK` and `ncp-live-cf`, and they
delete their items at the end, also after a failure. The questions show
on the terminal with `>>>`.

## Tests

| Filter | What it checks | What the maintainer does |
|---|---|---|
| `keychain` | `ncp` stores a secret in the login Keychain. Two different binaries read it with no dialog. A new binary stands for an update, which changes the code identity. | Nothing. If a dialog opens, click Deny: the test then fails. |
| `oauth` | A login at `mcp.cloudflare.com` with the browser and with `--no-browser`. The tokens go to the Keychain, `-vv` shows no token, and `server remove` deletes the tokens. | Log in to Cloudflare two times. Before each login, a `>>>` line tells the maintainer what to do, and when to wait. |
| `proxy` | With no proxy variable, `ncp` uses the HTTPS proxy of the system settings. The request goes through the tunnel of a test proxy to a local HTTPS server. | Nothing. It runs only with `NCP_DISPOSABLE_HOST=1`. |

The `oauth` test uses `https://mcp.cloudflare.com/mcp`. It is the only
third-party server of the live tests, and only the maintainer runs them.

### The system proxy

On macOS, only root can change the proxy of the system settings, and the
change is for the whole machine. Thus the `proxy` test runs only on a host
that is thrown away after the test, for example a CI runner, with
`NCP_DISPOSABLE_HOST=1` and `sudo` with no password. With no such
variable, `mise run live` skips it.

The other parts of the proxy route have tests that change nothing on the
machine, and the git hook runs them:

- The unit tests of `crates/ncp/src/proxy.rs` give the matcher a fake
  system proxy that copies hyper-util. They check the source that `-v`
  shows.
- The CLI tests start a test proxy, and set `HTTPS_PROXY` or `HTTP_PROXY`
  only for `ncp`. A request goes through the tunnel to a local HTTPS
  server.

## Results

| What | Where | Tracked |
|---|---|---|
| The terminal of each run | `.scratch/live/<date-time>.log` | No |
| A defect, or a fact that changes a decision | A new report `docs/research/live-tests-YYYY-MM-DD.md` | Yes |

A test that passes gives no report. The result applies only to one build.

The record can contain the account name of the maintainer and the URLs of the login, so it
stays in `.scratch/`. It also has the URL that the maintainer pastes in the `oauth`
test. The code in that URL works only one time, and only with the PKCE
verifier of that login.
