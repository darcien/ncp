# Conformance after elicitation

Date: 2026-10-09. This report adds to [conformance.md](conformance.md),
which stays as it is. Same suite, same Node, same method.

## Change

`ncp` now answers elicitation when stdin and stderr are a TTY (design
B63). The adapter acts as a person at a TTY: it takes each default, types
`y` where a value is necessary, and says yes to send. The test binary
gets the TTY through `NCP_TEST_TTY`, so the questions read from the pipe
of the adapter.

## Result

| Run | Passed checks | Failed checks | Baseline |
|---|---|---|---|
| Requirements 2025-11-25 | 392 | 16 | Passed |
| All scenarios 2025-11-25 | 338 | 1 | Passed |
| All scenarios 2025-11-25, era probe | 350 | 1 | Passed |
| Requirements 2026-07-28 | 592 | 16 | Passed |
| All scenarios 2026-07-28 | 536 | 1 | Passed |
| All scenarios 2026-07-28, era probe | 536 | 1 | Passed |

`elicitation-sep1034-client-defaults` and `sep-2322-client-request-state`
now pass, and left the list of expected failures. Two expected failures
stay, and both wait for a decision of the user:

- `auth/basic-cimd`: the place of the Client ID Metadata Document.
- `auth/pre-registration`: a client secret in the configuration
  (`docs/research/oauth-open-decisions.md`).

The 15 other failed checks of the requirement sets are extension
scenarios, which are out of scope.
