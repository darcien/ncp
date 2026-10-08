# OAuth login with a real browser: steps for the user

Date: 2026-10-08. Plan milestone 11.

## Question

Does `ncp auth login` work with a real browser, a real authorization
server, and the macOS Keychain? The automated tests use a fake browser, a
local authorization server, and the file store, because an agent cannot
use a browser or accept a Keychain dialog.

## Method

You need: a Mac, `ncp` from this repository, and an MCP server with OAuth
that you have an account for. Use a test account if you can.

1. Build `ncp`:

   ```
   cargo build -p ncp --release
   ```

2. Add the server to `~/.config/ncp/mcp.json`:

   ```json
   {"mcpServers": {"remote": {"type": "http", "url": "https://SERVER/mcp"}}}
   ```

3. Log in. The browser opens. Approve the login.

   ```
   ./target/release/ncp auth login remote
   ```

4. Check the login and call the server:

   ```
   ./target/release/ncp auth status remote
   ./target/release/ncp tool list remote
   ```

5. Repeat step 3 with `--no-browser`. Open the URL in a browser, approve,
   and paste the URL of the page that does not load.

6. Run step 4 with `-vv`, and look for a token value in the output:

   ```
   ./target/release/ncp -vv tool list remote 2>&1 | grep -i 'bearer [a-z0-9]'
   ```

7. Log out:

   ```
   ./target/release/ncp auth logout remote
   ```

## Expected result

- Step 3 prints "Logged in to remote at ISSUER" and shows no Keychain
  dialog.
- Step 4 shows "logged in", the issuer, the scopes and the expiry, and
  then the tools.
- Step 5 gives the same result as step 3.
- Step 6 finds no line.
- Step 7 says that the tokens were removed from the native store.

If step 3 fails with "the secret is too long for the security program",
the token set of the server is longer than the Keychain line limit (about
2900 bytes). Record the server: a split of the tokens into more items is
a decision for the user (design B51).

## Result

Not run yet. The user records the result here.
