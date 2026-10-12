# Live tests, 2026-10-12

The first runs of `mise run live` (`docs/live-tests.md`). The user ran
them on a Mac: macOS 15.7.4, arm64. The records are in `.scratch/live/`
(not tracked).

## Results

| Test | Build | Result |
|---|---|---|
| `keychain` | 5c9e508 | Pass. `ncp` and a second binary read the secret from the login Keychain. No dialog. |
| `oauth` | 5c9e508 | Fail. Both logins passed, and `-vv` showed the `Authorization` header only as `****`. Then `server remove cf` stopped with "configuration is not a JSON object": the defect below. |
| `oauth` | c4471d1 | Pass. Login with the browser and with `--no-browser`, tokens in the native store, `tool list`, no token in the trace, `server remove` removed the tokens, and `auth status` did not show `cf`. |
| `proxy` | 5c9e508 | Not run: the system proxy was not set. |

## Defect found and fixed

`server remove` of a project entry stopped when no global file existed. It
read the missing file as an empty text, which is not a JSON object. Fixed
in 5560df9, with a regression test. The other tests did not find it,
because each test of `server remove` had a global file.

## Open point: the scopes of a second login

The first login asked for 3 scopes: `user:read account:read
offline_access`. The second login, with the first tokens still valid,
asked for about 200: all `scopes_supported` of the server.

The cause: before the browser opens, `auth login` asks the server for its
`401` challenge, and the scopes of that challenge go into the request. The
probe sends the stored token. A valid token gets no `401`, so there is no
challenge, and the scope selection of MCP 2025-11-25 falls back to
`scopes_supported`.

Proposal: the probe of a login sends no stored token, because the login
replaces the tokens. The user has not decided yet.

## Not done

- `proxy`: run `mise run live proxy` with the system proxy set.
