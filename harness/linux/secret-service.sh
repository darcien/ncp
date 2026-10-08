#!/bin/sh
# Runs the Secret Service tests in the Linux harness container. Each case
# has its own HOME, D-Bus session and keyring daemon, so no case sees the
# state of another case. Never run this on a host: it starts a keyring
# daemon.
#
# Usage (in the container): harness/linux/secret-service.sh <target>
set -eu

target="$1"

# dbus-daemon stops when its user has no passwd entry. The image lets this
# script add one for the user of the run.
grep -q "^[^:]*:[^:]*:$(id -u):" /etc/passwd ||
	echo "ncp:x:$(id -u):$(id -g)::/tmp:/bin/sh" >>/etc/passwd
cargo build --target "$target" --locked -p ncp-testkit --test secret_service --quiet

# A session bus with no service activation. The normal session bus starts
# the keyring daemon on the first call, and then no case has "no Secret
# Service".
bus_config="$(mktemp)"
cat >"$bus_config" <<'XML'
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-BUS Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/tmp</listen>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
XML
session="dbus-run-session --config-file=$bus_config --"

run_case() {
	name="$1"
	shift
	echo "secret-service: $name"
	HOME="$(mktemp -d)" "$@" cargo test --target "$target" --locked --quiet \
		-p ncp-testkit --test secret_service -- --ignored --exact "$name"
}

# The keyring password comes on stdin. --unlock makes and unlocks the login
# keyring.
# The daemon owns its bus name a short time after it returns, so wait for
# the name, at most 5 s.
start_keyring='echo -n test-password | gnome-keyring-daemon --unlock --components=secrets >/dev/null && for i in 1 2 3 4 5 6 7 8 9 10; do dbus-send --session --print-reply --dest=org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus.ListNames | grep -q org.freedesktop.secrets && break; sleep 0.5; done'

run_case no_session_bus_gives_an_unavailable_store \
	env -u DBUS_SESSION_BUS_ADDRESS
run_case bus_with_no_secret_service_gives_an_unavailable_store \
	$session
run_case unlocked_keyring_keeps_and_lists_secrets \
	$session sh -c "$start_keyring && exec \"\$@\"" sh
run_case locked_keyring_gives_an_error_within_the_time_limit \
	$session sh -c "$start_keyring && exec \"\$@\"" sh
