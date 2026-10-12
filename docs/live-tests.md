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
```

The task builds `ncp`, and runs the tests in
`crates/testkit/tests/live.rs`. Each test uses its own temporary XDG
folders, so your configuration does not change. The questions show on the
terminal with `>>>`.

## Tests

| Filter | What it checks | What you do |
|---|---|---|
| `keychain` | `ncp` stores a secret in the login Keychain. Two different binaries read it with no dialog. A new binary stands for an update, which changes the code identity. | Nothing. If a dialog opens, click Deny: the test then fails. |
| `oauth` | A login at `mcp.cloudflare.com` with the browser and with `--no-browser`. The tokens go to the Keychain, `-vv` shows no token, and `server remove` removes the tokens. | Log in to Cloudflare two times, and paste one URL. |
| `proxy` | With no proxy variable, `ncp` uses the HTTPS proxy of the system settings, and `-v` names the source. A variable wins over the system settings. Nothing goes to the network: a fake proxy answers. | Set the system proxy before the test, and remove it after. |

The `oauth` test uses the server that you approved on 2026-10-11. The
test removes its Keychain items at the end, also after a failure.

### The system proxy

While the proxy is set, the HTTPS connections of the Mac fail. Set it
just before the test, and remove it just after:

```sh
networksetup -listallnetworkservices                              # Find the name, for example Wi-Fi.
sudo networksetup -setsecurewebproxy Wi-Fi 127.0.0.1 18080
mise run live proxy
sudo networksetup -setsecurewebproxystate Wi-Fi off
```

The test reads the port from `scutil --proxy`, and starts the fake proxy
on that port.

## Results

| What | Where | Tracked |
|---|---|---|
| The terminal of each run | `.scratch/live/<date-time>.log` | No |
| The result | A new report `docs/research/live-tests-YYYY-MM-DD.md` | Yes |

The record can contain your account name and the URLs of the login, so it
stays in `.scratch/`. Ask the agent to write the report from it.

These tests replace the steps of `docs/research/macos-keychain.md`,
`oauth-login-steps.md`, `macos-system-proxy.md`, and the OAuth step of
`server-add-interop.md`. Those reports are history.
