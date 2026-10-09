# Conformance as an agent with no TTY

Date: 2026-10-09. This report adds to
[conformance-elicitation.md](conformance-elicitation.md), which stays as
it is.

## Change

Since elicitation, the adapter acts as a person at a TTY. The path that
agents use, with no TTY, then had no run in the suite. `run.sh` now runs
all scenarios of each version a fourth time with `NCP_CONFORMANCE_AGENT`
set: the adapter gives no TTY and an empty stdin. The known failures of
this run are in `expected-failures-agent.yaml`.

## Result

| Run | Passed checks | Failed checks | Baseline |
|---|---|---|---|
| All scenarios 2025-11-25, as an agent | 333 | 7 | Passed |
| All scenarios 2026-07-28, as an agent | 532 | 4 | Passed |

The agent run has the two failures of the person run (CIMD and the client
secret, which wait for a decision of the user), and two more:

- `elicitation-sep1034-client-defaults` (2025-11-25 only).
- `sep-2322-client-request-state` (2026-07-28 only).

With no TTY, `ncp` declares no elicitation and gives `input_required` with
exit 5 (design B63). Thus these failures are the expected behavior.

## A fault in run.sh

`run.sh` set `NCP_CONFORMANCE_PROBE=1` before a call of a shell function.
In POSIX sh, such a variable stays set after the function returns. Thus
in earlier reports, all the runs of 2026-07-28 used the era probe of
`ncp`, also the runs that the table calls "requirements" and "all
scenarios". The numbers of those runs are correct for the probe path; the
path with the era that the adapter writes had no run for 2026-07-28. Each
run with a variable now runs in a subshell. With the fix, the
2026-07-28 runs give 592 and 536 passed checks, the same as before.
