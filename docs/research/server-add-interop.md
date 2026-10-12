# Interop test of `server add` and `server remove`

Date: 2026-10-12. Milestone 19 item 6 of [plan.md](../plan.md). The user
approved the three Cloudflare hosts on 2026-10-11. Only this manual test
uses them; the automated tests do not.

## Setup

- Build: commit e44dede and the hint of B78.
- Binary: the test build `target/debug/ncp-cli`. It runs the same code as
  `ncp`, with no native store, so the test cannot reach the macOS
  Keychain.
- `HOME` and the XDG folders in a scratch folder. The current folder is a
  folder with `.git` and no `.mcp.json`.

## Results

| Server | Step | Result |
|---|---|---|
| `https://docs.mcp.cloudflare.com/mcp` | `server add cf-docs URL` | Pass. "Created FILE." The new `.mcp.json` is in the current folder, with an indent of 2. `server list` shows it as trusted. The next steps are the login (conditional) and `server show`. |
| | `tool list cf-docs` | Pass. Two tools, both with the read-only hint. |
| | `tool call cf-docs search_cloudflare_documentation query="workers kv"` | Pass. Exit 0 with the result. |
| `https://blog.mcp.cloudflare.com/mcp` | `server add cf-blog URL` | Pass. The second entry comes after the first, and the order stays. |
| | `tool list cf-blog` | Pass. Four tools. |
| `https://docs.mcp.cloudflare.com` (no `/mcp`) | `server add cf-root URL` | Pass. The entry is written: `server add` does not contact the server. |
| | `server show cf-root` | First run: exit 3 with "HTTP 404 Not Found" and no hint. Fixed with B78: the error now has the hint to check the path of the URL. Second run: pass. |
| | `server remove cf-root` | Pass. The other entries and their order stay. |
| `https://mcp.cloudflare.com/mcp` | `server add cf URL` | Pass. JSON result with `scope`, `file`, `created`, `changed`, empty `secrets`, and the next steps `ncp auth login cf` ("when": "the server needs a login") and `ncp server show cf`. |

## Not done: the OAuth step

The OAuth step of `mcp.cloudflare.com` needs a browser and the account of
the user. The user does these steps on a Mac, with the release build of
`ncp`:

1. In a test folder with `.git`, run `ncp server add cf https://mcp.cloudflare.com/mcp`.
2. Run `ncp auth login cf`. The browser opens. Log in with your account,
   and give the access.
3. Run `ncp tool list cf`. Expected: the tools of the server.
4. Run `ncp server remove cf`. Expected: "Removed the OAuth tokens of cf
   from the native store." (or the credential file).
5. Run `ncp auth status`. Expected: no `cf`.

Record the result in a new report.
