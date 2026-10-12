#!/bin/sh
# Runs the live tests on this Mac: the login Keychain, a real browser login
# and the system proxy. A person must be present. See docs/live-tests.md.
#
# Usage:
#   harness/live/run.sh [FILTER]
#
# FILTER selects tests by name, for example "keychain". With no FILTER, the
# proxy test does not run: while the system proxy points to the test, the
# other HTTPS connections of the Mac fail. Run it alone with "proxy".
#
# The terminal goes to a record in .scratch/live/, through script, so that
# the questions of ncp still see a terminal.
set -eu

if [ "$(uname)" != Darwin ]; then
	echo "run.sh: the live tests run only on macOS" >&2
	exit 2
fi

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
mkdir -p .scratch/live
record=".scratch/live/$(date +%Y-%m-%d-%H%M%S).log"

if [ $# -eq 0 ]; then
	set -- --skip system_proxy
fi

cargo build --locked -p ncp
status=0
script -q "$record" cargo test -p ncp-testkit --locked --test live -- \
	--ignored --test-threads=1 --nocapture "$@" || status=$?
echo "Record: $record"
exit "$status"
