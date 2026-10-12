#!/bin/sh
# Runs the live tests on this Mac: the login Keychain and a real browser
# login. See docs/live-tests.md.
#
# Usage:
#   harness/live/run.sh [FILTER]
#
# FILTER selects tests by name, for example "keychain". The terminal goes
# to a record in .scratch/live/, through script, so that the questions of
# ncp still see a terminal.
set -eu

if [ "$(uname)" != Darwin ]; then
	echo "run.sh: the live tests run only on macOS" >&2
	exit 2
fi

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
mkdir -p .scratch/live
record=".scratch/live/$(date +%Y-%m-%d-%H%M%S).log"

# The proxy test changes the proxy of the whole machine, so it runs only on
# a host that is thrown away after the test, for example a CI runner. A
# FILTER that selects it runs it, and the test then stops with the reason.
if [ $# -eq 0 ] && [ "${NCP_DISPOSABLE_HOST:-}" != 1 ]; then
	echo "run.sh: the proxy test runs only with NCP_DISPOSABLE_HOST=1" >&2
	set -- "$@" --skip system_proxy
fi

cargo build --locked -p ncp
status=0
script -q "$record" cargo test -p ncp-testkit --locked --test live -- \
	--ignored --test-threads=1 --nocapture "$@" || status=$?
echo "Record: $record"
exit "$status"
