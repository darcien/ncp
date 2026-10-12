# Live test of the login scopes, 2026-10-12

The run of `mise run live oauth` after the change of the login scopes
(design B79). The user ran it on the same Mac as
`live-tests-2026-10-12.md`. Build: 9804030. Record:
`.scratch/live/2026-10-12-235628.log` (not tracked).

## Result

Pass. Both logins asked for `user:read account:read offline_access`: the
scope of the `401` challenge and `offline_access`. Before the change, the
second login asked for about 200 scopes.

## Finding

The questions of the test were not clear: the user could not see when to
act and when to wait. The JSON output of `ncp` also made the login URL
hard to read. The test now runs the logins with text output, and each
question says what to do and when to wait.
