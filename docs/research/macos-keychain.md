# The macOS Keychain through `/usr/bin/security`

Date: 2026-10-08. This report replaces the macOS parts of
`credential-store.md`. It does not change the Linux parts.

## Question

How does `ncp` use the login Keychain through `/usr/bin/security`, with no
secret in a process argument? Which limits and exit codes must the store
know?

## Background

`ncp` has no paid Developer ID. An ad-hoc signed binary changes its code
identity at each update. The Keychain trusts the program that made an item.
Through the native API, it asks again for each item after an update, and an
agent cannot answer the dialog. An item that `/usr/bin/security` makes
trusts `/usr/bin/security`. gh has the same problem and uses the same
method ([go-keyring][gk]). The user accepted the cost on 2026-10-07: a
process of the same user can read the items of `ncp` through `security`
with no prompt.

## Method

I read `man security` and `security -h` for the three commands. I sent
lines to `security -i` with the `help` command only, because `help` does not
open a keychain. No step read, changed or searched the real Keychain.
macOS 15.7.4, arm64.

## Result

| Topic | Result | Source |
|---|---|---|
| Value in an argument | `-w <password>` puts the value in the arguments, which each user can see. `-w` as the last option asks on the terminal, which an agent does not have. | `security -h add-generic-password` |
| Interactive mode | `security -i` reads commands from stdin until the end of the input. It shows no prompt when stdin is not a terminal. | `man security`, go-keyring |
| Quotes | A double quote at the start of a word quotes the word up to the next double quote. A backslash keeps the next character as it is, also in a quoted word. A double quote in the middle of a word is a usual character. Single quotes, `#`, `$` and spaces in a quoted word are usual characters. | probes with `help` |
| Line buffer | A line of 4096 bytes or less, with its new line, is one command. The program runs the rest of a longer line as a new command. This agrees with `fgets` and a buffer of 4096 bytes. | probes with `help` |
| Exit code of `-i` | The exit code is the code of the last command. `help zzz` gives 1. | probe with `help` |
| Exit codes | The program exits with the low byte of the status of the Security framework. errSecItemNotFound (-25300) gives 44. The other codes in the store come from the same rule: 36, 37, 50, 51 and 128. | go-keyring, arithmetic |
| Output of `-w` | `find-generic-password -w` shows a value that has a byte that is not printable ASCII as hexadecimal digits. | go-keyring |

## Decision

- Write: send `add-generic-password -U -s "ncp" -a "<account>" -w
  "<encoded>"` to `security -i` through stdin.
- Encode each value as `ncp-base64:` and the base64 of the value. The
  encoded value has only printable ASCII and no quote, so it is safe in
  the command and in the output.
- Read: `find-generic-password -s ncp -a <account> -w`. Exit code 44 means
  no item. A value with no `ncp-base64:` mark is plain text.
- Delete: `delete-generic-password -s ncp -a <account>`. Exit code 44 is
  not an error.
- Exit codes 36, 51 and 128 give "access denied". Exit codes 37 and 50
  give "no store". Other codes give a failure. The error of a write does
  not show stderr, because stderr can hold the command.
- The store refuses a command line longer than 4095 bytes with its new
  line, before it starts the program. A shorter limit also prevents a
  last empty command that can change the exit code.
- The store does not use the native API and has no macOS dependency. Tests
  run a fake `security` program from the test kit on each platform.

## Check on a real Mac

These steps are for the user. An agent must not do them, because the real
Keychain can show a dialog that an agent cannot accept. Before `ncp` has the
`secret` commands, do part A only.

Part A, the method, with `security` only:

1. Save a test item with the same command that `ncp` sends:

   ```sh
   printf 'add-generic-password -U -s "ncp" -a "check/manual" -w "ncp-base64:%s"\n' \
     "$(printf 'check value' | base64)" | /usr/bin/security -i
   echo "exit code: $?"
   ```

   Make sure that the exit code is 0.

2. Read the item:

   ```sh
   /usr/bin/security find-generic-password -s ncp -a check/manual -w
   ```

   Make sure that no dialog appears and that the output starts with
   `ncp-base64:`.

3. Look at the item list with no data:

   ```sh
   /usr/bin/security dump-keychain | grep -B4 -A4 '"svce"<blob>="ncp"'
   ```

   Do not add `-d`. Make sure that no dialog appears. Record the result:
   it decides how `secret list` finds the names.

4. Remove the test item:

   ```sh
   /usr/bin/security delete-generic-password -s ncp -a check/manual
   ```

Part B, with `ncp`, when the `secret` commands exist:

1. Build `ncp` and save a secret: `printf 'check value' | ncp secret set
   NCP_CHECK`.
2. Make a small change to the code, for example in a comment. Build `ncp`
   again, so that the binary gets a new ad-hoc signature.
3. Run a command that reads `${NCP_CHECK}`, and run `ncp secret list`. Make
   sure that no dialog appears.
4. Remove the secret: `ncp secret delete NCP_CHECK`.

## Open points

- A value that is longer than approximately 2900 bytes does not fit in one
  command line. An OAuth token set with a large access token can be longer.
  Measure the token sets of real servers before the OAuth milestone.
- An item that an earlier `ncp` made with the native API trusts that
  earlier binary. A read or an update of that item through `security` can
  show a dialog. Remove such items by hand.
- The store reads and writes the default keychain. On almost all Macs this
  is the login Keychain.
- Does `security dump-keychain` with no `-d` show a dialog? Part A, step 3
  answers this.

[gk]: https://github.com/zalando/go-keyring/blob/master/keyring_darwin.go
