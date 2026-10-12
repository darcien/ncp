# Live tests

The live tests run `ncp` against the real parts of this Mac and a real
server: the login Keychain, a browser, the system settings, and a login
with your account. The other tests are sealed: they use only local stubs,
fixtures and temporary folders. A live test needs a person, so only the
user runs them, and the git hook only compiles them.

Run them before a release, and after a change to the store, the login or
the proxy code.

## Run

```sh
mise run live            # All tests.
mise run live keychain   # One test. The filter is a part of the test name.
mise run live proxy      # The proxy test only.
```

The task builds `ncp`, and runs the tests in
`crates/testkit/tests/live.rs`. Each test uses its own temporary XDG
folders, so your configuration does not change. The Keychain is shared:
the tests use only the names `NCP_LIVE_CHECK` and `ncp-live-cf`, and they
remove their items at the end, also after a failure. The questions show
on the terminal with `>>>`.

## Tests

| Filter | What it checks | What you do |
|---|---|---|
| `keychain` | `ncp` stores a secret in the login Keychain. Two different binaries read it with no dialog. A new binary stands for an update, which changes the code identity. | Nothing. If a dialog opens, click Deny: the test then fails. |
| `oauth` | A login at `mcp.cloudflare.com` with the browser and with `--no-browser`. The tokens go to the Keychain, `-vv` shows no token, and `server remove` removes the tokens. | Log in to Cloudflare two times. Before each login, a `>>>` line tells you what to do, and when to wait. |
| `proxy` | With no proxy variable, `ncp` uses the HTTPS proxy of the system settings, and `-v` names the source. The request goes through the tunnel of a test proxy to a local HTTPS server. A variable wins over the system settings. Nothing goes to the network. | Type your password for `sudo`. |

The `oauth` test uses the server that you approved on 2026-10-11.

### The system proxy

The test changes the system settings itself, with `sudo networksetup`:

1. It stops when a network service uses an HTTPS proxy already, for
   example a company proxy. It does not change such a proxy.
2. It starts a test proxy on a free port of 127.0.0.1, and sets the HTTPS
   proxy of each network service that is on to that port.
3. After the checks, it sets the proxy of each service off again, also
   after a failure. The settings keep 127.0.0.1 and the port, with the
   proxy off.

While the test runs (some seconds), the other HTTPS connections of the
Mac fail. If you stop the test with Ctrl-C, set the proxy back yourself:

```sh
networksetup -listallnetworkservices
sudo networksetup -setsecurewebproxystate SERVICE off
```

The test needs no person and no network, only `sudo`. A CI runner on
macOS has `sudo` with no password, so it can run `mise run live proxy`.

## Results

| What | Where | Tracked |
|---|---|---|
| The terminal of each run | `.scratch/live/<date-time>.log` | No |
| The result | A new report `docs/research/live-tests-YYYY-MM-DD.md` | Yes |

The record can contain your account name and the URLs of the login, so it
stays in `.scratch/`. It also has the URL that you paste in the `oauth`
test. The code in that URL works only one time, and only with the PKCE
verifier of that login. Ask the agent to write the report from the record.

These tests replace the steps of `docs/research/macos-keychain.md`,
`oauth-login-steps.md`, `macos-system-proxy.md`, and the OAuth step of
`server-add-interop.md`. Those reports are history.
