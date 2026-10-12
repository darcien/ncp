# Manual tests

The tests that an agent cannot do: they need the real Keychain, a real
browser, the system settings of the Mac, or a login with your account.
Do them on a Mac before milestone 20 (distribution).

This file replaces the steps in these reports:

- `docs/research/macos-keychain.md`, parts A and B. Part A checked the
  method before `ncp` had the `secret` commands. Part B step 1 is old:
  since milestone 17, a secret must have an entry that names `${NAME}`.
- `docs/research/oauth-login-steps.md`.
- `docs/research/macos-system-proxy.md`.
- `docs/research/server-add-interop.md`, the OAuth step.

Do not record results in those reports. They are history.

## Where the output goes

| What | Where | Tracked |
|---|---|---|
| The terminal output of each test, with your notes | `.scratch/manual/<test>.txt` | No |
| The result of the run | A new report `docs/research/manual-tests-YYYY-MM-DD.md` | Yes |

Each test runs in `script`, which records the terminal as you see it. The
output can contain your account name, hosts and the URLs of the login, so
it stays in `.scratch/`. To write a note in the record, type a line that
starts with `:`, for example `: no dialog`. The shell does nothing with
it.

After a run, ask the agent to write the report from the records, or write
it yourself. Give one line for each test: pass or fail, and what you saw.

## Setup

Do this one time in the terminal, in the root of the repository. The XDG
folders go to `.scratch/manual/home`, so the tests do not change your own
configuration. The Keychain is the real login Keychain.

mise also uses the XDG folders: for its trust records and its global
configuration. The `MISE_*_DIR` lines keep the mise folders where they
are, else mise does not trust `mise.toml` in a new shell, and the tools
of mise (for example Node) are not on `PATH`.

```sh
cargo build --release --locked -p ncp
export PATH="$PWD/target/release:$PATH"
export MISE_CONFIG_DIR="$HOME/.config/mise"
export MISE_STATE_DIR="$HOME/.local/state/mise"
export MISE_CACHE_DIR="$HOME/Library/Caches/mise"
export NCP_MANUAL="$PWD/.scratch/manual"
export XDG_CONFIG_HOME="$NCP_MANUAL/home/config"
export XDG_STATE_HOME="$NCP_MANUAL/home/state"
export XDG_CACHE_HOME="$NCP_MANUAL/home/cache"
mkdir -p "$NCP_MANUAL"
{ git rev-parse --short HEAD; ncp --version; sw_vers; } >"$NCP_MANUAL/setup.txt"
```

## 1. Keychain

Question: does `ncp` read and write the Keychain with no dialog, also
after the binary changes? The binary has no Developer ID, so each build
has a new code identity. The items trust `/usr/bin/security`, not `ncp`
(report `macos-keychain.md`).

The test server is the MCP everything server of the interop tests. Its
tool `get-env` returns the environment of the server. The `jq` filter
shows only `NCP_CHECK`, because the other variables can hold your own
secrets.

```sh
mise run interop:install   # Only if harness/interop/node_modules is missing.
cargo build --locked -p ncp
script -a "$NCP_MANUAL/keychain.txt"
```

In the `script` shell:

```sh
server="$PWD/harness/interop/node_modules/.bin/mcp-server-everything"
printf 'check value' | ncp server add check --global --env NCP_CHECK --secret NCP_CHECK -- "$server" stdio
ncp secret list
ncp --output-format json tool call check get-env | jq -r '.content[0].text | fromjson | .NCP_CHECK'
: dialog: yes or no

# The debug build is a different binary with a different code identity.
target/debug/ncp secret list
target/debug/ncp --output-format json tool call check get-env | jq -r '.content[0].text | fromjson | .NCP_CHECK'
: dialog: yes or no

ncp server remove check
ncp secret delete NCP_CHECK --all
ncp secret list
exit
```

Expected:

- `server add` says that it stored `NCP_CHECK` in the native store.
- `secret list` shows `NCP_CHECK` in the native store, used by `check`.
- The `jq` lines show `check value`, from both binaries.
- No dialog at any step.
- At the end, `secret list` does not show `NCP_CHECK`.

## 2. OAuth login with a real browser

Question: does `ncp auth login` work with a real browser, a real
authorization server and the Keychain? The server is
`mcp.cloudflare.com`, which you approved on 2026-10-11. Use your
Cloudflare account.

```sh
mkdir -p "$NCP_MANUAL/project" && cd "$NCP_MANUAL/project" && git init -q
script -a "$NCP_MANUAL/oauth.txt"
```

In the `script` shell:

```sh
ncp server add cf https://mcp.cloudflare.com/mcp
ncp auth login cf
: browser: ...; dialog: yes or no
ncp auth status cf
ncp tool list cf

ncp auth login cf --no-browser
: paste: ...
ncp -vv tool list cf 2>&1 | grep -ic 'bearer [a-z0-9]'

ncp server remove cf
ncp auth status
exit
```

Then go back with `cd -`.

At `auth login cf`, the browser opens. Give the access. At `--no-browser`,
open the URL in a browser, give the access, and paste the URL of the page
that does not load.

Expected:

- `server add` creates `.mcp.json` in the project folder. The next steps
  name `ncp auth login cf`.
- `auth login` says "Logged in to cf at ISSUER. The tokens are in the
  native store." No dialog.
- `auth status cf` shows the issuer, the scopes and the expiry.
  `tool list cf` shows the tools.
- `--no-browser` gives the same result as the browser login.
- The `grep` count is `0`: `-vv` shows no token.
- `server remove cf` says that it removed the OAuth tokens of `cf` from
  the native store.
- At the end, `auth status` does not show `cf`.

A token set longer than about 2900 bytes goes into several Keychain items
(design B66). If a server gives such a token set, the steps are the same.

## 3. System proxy

Question: does `ncp` use the manual HTTP and HTTPS proxy of the system
settings when no proxy variable is set? `ncp` reads only the manual
proxy, not PAC and not the bypass list (design C5).

You need a proxy that logs each connection. The example uses `mitmdump`.
With `--ignore-hosts '.*'`, TLS goes through with no change, so `ncp` can
check the certificate of the server. In a second terminal:

```sh
mitmdump --listen-port 8080 --ignore-hosts '.*'
```

Set System Settings > Network > (your network) > Details > Proxies >
"Web proxy (HTTP)" and "Secure web proxy (HTTPS)" to `127.0.0.1`, port
`8080`. Then:

```sh
script -a "$NCP_MANUAL/proxy.txt"
```

In the `script` shell:

```sh
ncp server add cf-docs --global https://docs.mcp.cloudflare.com/mcp
unset HTTP_PROXY HTTPS_PROXY ALL_PROXY http_proxy https_proxy all_proxy NO_PROXY no_proxy
ncp -v tool list cf-docs
: proxy log: connection to docs.mcp.cloudflare.com: yes or no
HTTPS_PROXY=http://127.0.0.1:9 ncp -v tool list cf-docs
ncp server remove cf-docs
exit
```

Turn the two proxies off again in System Settings.

Expected:

- The first run shows `route to https://docs.mcp.cloudflare.com/mcp:
  proxy http://127.0.0.1:8080/ from the system settings`, and the tools.
  The proxy log shows a connection to `docs.mcp.cloudflare.com:443`.
- The second run names `HTTPS_PROXY` as the source, and fails because
  nothing listens on port 9. When a variable is set, the system settings
  have no effect.

## After the tests

```sh
rm -rf "$NCP_MANUAL/home" "$NCP_MANUAL/project"
```

Keep the `.txt` files until the report is written.
