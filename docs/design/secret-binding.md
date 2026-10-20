# Design: bind stored secrets to origins

> Archived at v0.1.0. This MVP document describes v0.1.0 and gives no
> direction. When the code and this document do not agree, the code is
> correct.

Status: accepted by the user on 2026-10-10 (revision 5). Requirement S9
and [cli.md](cli.md) B72 record the rules. Build: [plan.md](../plan.md)
milestone 17.

Context: [requirements.md](../requirements.md) §3.3. [cli.md](cli.md) §2.4,
C1, C1a, B68, B70. Research: [add-command.md](../research/add-command.md)
§7, [pnpm-credentials.md](../research/pnpm-credentials.md).

Terms in this document:
- **Origin**: the scheme, host and port of a URL (rule 1).
- **Destination**: the party that gets a value. For a stored value, this
  is an origin, or no destination for a stdio server (rule 2).
- **Store key**: the account of an item in the store (rule 4).

## 1. Today

- A secret is a named value (C1). The store keeps one item for each name,
  with the account `secret/NAME`. Each entry that names `${NAME}` gets the
  value.
- The environment comes before the store (C1a).
- `ncp` binds OAuth tokens to the server: the canonical URL and the issuer
  (A13, B70). Static secrets have no binding.

## 2. Problem

Any entry can name any stored `${NAME}`. These accidental leaks are in the
threat model (§3.3):

| # | Path | Example |
|---|---|---|
| P1 | Name collision | Two READMEs use `${API_KEY}`. The second server gets the key of the first server. With `server add`, this leak needs only one copied line. |
| P2 | URL change | The user changes the URL of an entry, for example from staging to production, or with a typo in the host. The secret goes to the new URL. |
| P3 | Another file | A `--config-file` names `${GITHUB_TOKEN}` with another URL. |
| P4 | Copied OAuth entry | A copied entry has another URL and `oauth.clientSecret: "${CLIENT_SECRET}"`. |

Out of scope:
- An agent with a shell that an attacker controls. The agent can read the
  store (C1b).
- A hostile server at a bound origin.
- The code of a stdio server. This code runs as the user and can read the
  store.

The binding applies only to values from the store. This design accepts
these risks:
- **Environment values.** A value from the environment has no binding, in
  all files (C1a). P1 to P4 stay open for this value. For example, a
  README tells the user to run `export LINEAR_KEY=...`. Reason: `ncp`
  cannot know the destination that the user intends for a variable. A
  stored value is the safe path, and the docs of `ncp` show it first.
- **A `--config-file` that the user did not write.** An example is a file
  in a cloned repository that names environment values. S5 adds the
  project files that `ncp` finds by itself. The S5 design uses the pnpm
  rule: no `${NAME}` from the environment in a credential field of a
  project file ([pnpm-credentials.md](../research/pnpm-credentials.md) §1).
- **stdio `env` values.** A stdio server of one vendor can get the key of
  another vendor. The server can then send this key to its own API by
  accident. A stdio value has no destination that `ncp` can compare.
- **Servers of different authors on one host.** An example is a gateway
  that serves each server under its own path. These servers have the same
  origin, so they can use the same stored value. Reason: the origin is the
  security boundary of the web (RFC 6454). docker and gh use the host as
  the key of a credential, and git does the same by default
  ([add-command.md](../research/add-command.md) §7). The question and the
  result of `secret set` list each server that uses the item. An entry
  that the user adds later at the same origin gets the value with no
  question.

## 3. Decision

A stored value goes only to the origin in its store key. A lookup for
another origin finds no item. Thus no code path can skip the check for a
stored value.

A new binding needs the value. On a TTY, the user types the value after a
question that shows the origin. From a pipe, the user gives the value, and
the result shows the origin.

Reasons:
- Credential tools use the destination as the key of a credential. They
  send the credential only to this destination: git, docker, gh and uv
  ([add-command.md](../research/add-command.md) §7). pnpm does the same:
  `_auth` "is keyed by registry URL, so each secret is explicitly bound to
  the host that may receive it"
  ([pnpm-credentials.md](../research/pnpm-credentials.md) §1).
- The MCP spec binds OAuth tokens to an audience (RFC 8707). It binds
  pre-registered client credentials to their issuer (MCP 2026-07-28 client
  registration, Authorization Server Binding). `ncp` does the same (A13,
  B68).
- A secure default must fail closed and give one clear fix. pnpm does this
  for build scripts (`strictDepBuilds`, default true): the install stops,
  and one command records the decision. On a miss, `ncp` stops before the
  send, and the hint gives one command.
- `ncp` does not copy a stored value to a new origin. With a copy, the
  typo leak of P2 needs only one key press ("Send the value to
  https://mcp.linear.ap? y"). Also, staging and production usually have
  different keys. To use the same value again, the user gets it from the
  vendor again, or from the place where the user keeps it (for example a
  password manager). The macOS Keychain shows only an encoded value
  (`ncp-base64:` or `ncp-parts:`), so Keychain Access is not a source.
- A change of the store format has a low cost, because there is no release
  yet.

This decision replaces one part of the credential review of 2026-10-09:
"no change of the store format". The other part stays: there is no
`--origin` flag. The origins come from the configuration.

## 4. Rules

1. **Origin.** The origin is the scheme, host and port of the entry URL
   (RFC 6454). It does not include the path, because a server can move
   from `/mcp` to `/v2/mcp`. `ncp` writes the origin in the ASCII
   serialization of the WHATWG URL Standard:
   - a lower-case host,
   - an IDNA host as `xn--` labels,
   - an IPv6 address in brackets,
   - no default port.

   A trailing dot stays, so `host.` is a different origin.
2. **Destination of a value.**
   - A `headers` value of an `http` entry: the origin of the entry URL.
   - The `oauth.clientSecret` value of an `http` entry: the origin of
     `oauth.issuer`. B68 makes `oauth.issuer` necessary with a client
     secret, and it is literal text in the file. The secret goes only to
     the token endpoint in the metadata of this issuer. rmcp makes sure
     that the issuer in the metadata is the issuer of the discovery URL
     (RFC 8414 §3.3). Then B68 compares this issuer with `oauth.issuer`.
     B68 does this before the browser opens and before each token request.
     Reasons:
     - The store key names the real destination, so `secret list` shows
       it.
     - A header at another origin finds no item, so a client secret cannot
       go to an MCP server as a header.
     - A server that moves to a new URL and keeps its issuer keeps its
       client secret. Many vendors show a client secret only one time.
   - An `env` value of a `stdio` entry: no destination (section 2).
3. **Literal origin.** When an entry uses a stored value, the scheme, host
   and port in its `url` must not contain `${`. `ncp` reads the origin from
   the URL as the file writes it. The origin ends at the first `/`, `?` or
   `#` after the host, or at the end of the URL. If a `${` comes before
   this point, the entry has no origin. A stored value for this entry then
   gives a usage error (exit 2). The message tells the user to write the
   scheme, host and port of `url` as literal text. It does not tell the
   user to export the secret, because an environment value has no binding.
   The same rule applies to `oauth.issuer`. Reasons:
   - An environment value can change the host, for example in
     `https://mcp.example.com${SFX}/mcp`.
   - Each message and `secret list` show the origin. Output must show no
     expanded value (B13, S3).
4. **Store key.** The store key of an item has one of two forms:

   ```
   account = "secret/" NAME
           | "secret/" ORIGIN "/" NAME
   ORIGIN  = scheme "://" host [":" port]    (normal form of rule 1)
   NAME    = [A-Za-z_][A-Za-z0-9_]*          (SecretName today)
   ```

   An example is `secret/https://mcp.linear.app/LINEAR_KEY`. The opt-in
   file store uses the same keys. `ncp` does not encode the value.

   `ncp` parses an account in these steps:
   1. Remove the prefix `secret/`.
   2. If the rest has no `://`, the rest must be a NAME. This is an item
      with no destination.
   3. Else, split the rest at the first `/` after `://`. The text before
      the split is the origin. The text after the split must be a NAME.
   4. Parse the origin again with rule 1. If the result is not the same
      text, the account is not an item.

   Each other account gives no key, as today. These facts make the grammar
   unambiguous:
   - A NAME has no `/` and no `:`.
   - An origin in normal form has no path, so its first `/` after `://`
     ends it.
   - Thus `secret/https` (a stdio item with the NAME `https`) is not a
     prefix of `secret/https://...`.

   `ncp` does not split at the last `/`. A test with the url crate 2.5.8
   (2026-10-10) showed that such a split accepts accounts that are not in
   normal form. Examples are `secret/HTTPS://MCP.LINEAR.APP/K`,
   `secret/https://h:443/K` and `secret/https://h/mcp/K`. Then one origin
   can have two items. The same test showed these facts:
   - A part account of B66 (`ACCOUNT/GENERATION/INDEX`) gives no key.
   - No account is another account followed by `/`. Thus the clean-up of
     the parts of one item never removes another item.
   - The longest account is 334 bytes, and its longest part account is 381
     bytes. Both fit in one line of `security -i` (4095 bytes).

   Only `CredentialKey::account` and `CredentialKey::from_account` know
   this text form. Reasons for `/` as the separator:
   - The accounts `oauth/SERVER/tokens` and the B66 parts already use `/`.
   - Other credential tools also put the destination as text in the key:
     uv uses a URL as the keyring service, gh uses `gh:HOST`, and pnpm
     uses the registry URL ([add-command.md](../research/add-command.md)
     §7).
   - A Keychain generic password is unique by service and account only.
     Thus the origin must be part of the account, not a separate
     attribute. git uses separate attributes, but it uses Internet
     password items. These items show in the Passwords app of the user.
   - Percent-encoding of the origin is also unambiguous, but the user then
     cannot read the account in Keychain Access.
5. **Lookup.** The environment comes first (C1a). Then `ncp` reads the item
   of the destination: first the native store, then the file. No other
   item is a fallback. A miss gives the error of an unset `${NAME}` (exit
   2), with the origin:

   ```
   error: The variable `LINEAR_KEY` is not set, and no store has a value for `https://mcp.linear.ap`. ncp sent nothing to the server `linear`.

   hint: If the URL is correct, run `ncp secret set LINEAR_KEY --server linear` to store the value

   hint: To see the stored items and their destinations, run `ncp secret list`
   ```

   In this example, `mcp.linear.ap` is a typo in the host: the last `p` is
   missing (the P2 case). The message says what is missing. The condition
   "If the URL is correct" is in the hint. Each hint is a sentence. A hint
   that only reads state or starts a login has the JSON field `command`,
   because agents run a hint command (B3, B10). The JSON error has the field
   `destination`. The message names only the current origin. `ncp` reads
   no other item to write the message. When the native store cannot
   answer, the hint is `ncp secret set NAME --server SERVER
   --credential-store file`.
6. **`secret set NAME [--server SERVER]`**
   1. `ncp` finds the entries that name `${NAME}` in `env`, `headers` or
      `oauth.clientSecret`. With `--server`, `ncp` uses only that entry.
      If the configuration does not load, the exit code is that of B22. If
      no entry names `${NAME}` (also with `--server`), `ncp` gives a usage
      error, and gives a hint that tells the user to add the entry first. If
      `${NAME}` is only in `command`, `args`, `cwd` or `url`, the hint is
      that of B32. (After milestone 19, the hint also gives `server add
      --secret`.)
   2. The targets are the different store keys (rule 4) of these entries.
      If there are two or more targets, `ncp` gives a usage error. The
      error lists each server and its destination. The hint is `--server`.
   3. On a TTY, the question shows the destination and the servers that
      use it. An example is "Value of LINEAR_KEY for https://mcp.linear.app
      (server linear):". For a client secret, the question says "the
      issuer of server acme". A host with an `xn--` label gets the warning
      of B63. From a pipe, `ncp` reads the value and asks no question.
   4. `ncp` writes the item of the target and replaces an old item. The
      result shows the place, the destination and the servers. An example
      is "Stored LINEAR_KEY in the native store for https://mcp.linear.app
      (server linear)." For a stdio target, the destination text is "for
      stdio servers", and the JSON `destination` is `null`.

   A script that changes a key must give `--server`. Without it, the
   script fails when the user adds a second entry with the same name.
7. **`secret delete NAME [--server SERVER | --unused | --all]`**
   - With `--server`, `ncp` uses the target rule of `secret set`. It
     removes the item of that destination from each place.
   - With `--unused`, `ncp` removes the items of NAME that no entry uses,
     for example after a URL change. `ncp` looks only at the loaded file:
     the default file or `--config-file`. If the file does not load, or an
     entry is invalid, `ncp` gives a usage error. Reason: `ncp` cannot
     know if an invalid entry uses NAME.
   - With `--all`, `ncp` removes all items of NAME.
   - With no flag, `ncp` removes the item when NAME has only one
     destination. If NAME has two or more destinations, `ncp` gives a
     usage error that lists them, and removes nothing. Reason: the user
     cannot get a value back through `ncp`.

   The result names the file that `ncp` read, and lists each removed
   destination and place. `server remove` changes no secret.
8. **`secret list`** reads only the accounts (B38). It shows one row for
   each name and store key: `{"name", "destination", "credentialStores", "used",
   "servers"}`. `destination` is `null` for stdio. `used` means that an
   entry names `${NAME}` with that store key.
9. **Messages.** Each message shows a URL as the file writes it, never the
   expanded URL (B13). `-v` shows the destination of each stored value,
   for example "LINEAR_KEY from the native store, for
   https://mcp.linear.app".

## 5. Changes in other documents (requirements first)

Done on 2026-10-10: requirements S9 and I8, and `cli.md` (C1, §2, §2.3,
§2.4, §4, B27, B35, B51, B66, B68, B72). Milestone 19 does the changes to
the `server add` proposal.

- [requirements.md](../requirements.md):
  - §3.3 gets S9, a new MUST: "`ncp` uses the destination of a
    stored static secret as part of its key. A header value goes only to
    the origin of its entry (no redirects, N6). A client secret goes only
    to the token endpoint of the configured issuer, and its key has the
    origin of this issuer. When no item exists for the destination, `ncp`
    stops before the send."
  - I8 gets this rule: when an entry uses a stored value, its URL and its
    `oauth.issuer` have no `${NAME}` in the scheme, host or port.
- [cli.md](cli.md):
  - C1 and §2.4: a secret is a named value for one origin. They get the
    lookup and the error of rule 5. The examples add the entry first, and
    then run `secret set`.
  - Sections 1 and 2 get `--server` on `secret set`, and `--server`,
    `--unused` and `--all` on `secret delete`.
  - Section 4: the `secret list` JSON has one object for each name and
    store key. B27: the result of `secret set` gets `destination` and
    `servers`.
  - B35: the check for a copy in the other place uses the same key. The
    warning names the destination, and its hint gives `--server`.
  - B27: the text result of `secret set` keeps the place.
  - B51: a request can send a stored header value and get `401`. Then the
    first hint is `ncp secret set NAME --server SERVER`. The second hint is
    `ncp auth login SERVER`. Reason: after the user changes the key at one
    origin, the key at another origin can be old.
  - B66 gets the store key form of rule 4.
  - B68: the key of a client secret has the origin of `oauth.issuer`.
  - B72 records rules 6 and 7.
- [plan.md](../plan.md), milestone 17. The tests:
  - the origin form: case, IDNA, IPv6, default port, trailing dot, `http`
    and `https`
  - the literal origin of rule 3, with userinfo, a port variable and a
    path variable
  - the lookup miss
  - `secret set` with no entry, one target and two targets, `--server`, a
    TTY and a pipe
  - `secret delete` with `--server`, `--unused`, `--all` and no flag
  - the store key: a round trip for each origin case, part accounts that
    give no key, accounts that are not in normal form, and the prefix rule
    against the B66 clean-up
  - a metadata issuer that is not the issuer of the discovery URL
  - a header `${NAME}` that names a client secret, at the origin of the
    MCP server
  - the in-memory store and the file store.
- The `server add` proposal (`.scratch/backlog/server-add.md`):
  - The two-command form adds the entry first, and then runs `secret set`.
  - `--secret` writes the item of the new entry. The result gets
    `destination`.
  - Only entries with the same origin use the same stored value. Change
    rule 13, the `usedBy` field and section 4.3 to match.
  - `--env NAME` shows the other stdio entries that use NAME.
