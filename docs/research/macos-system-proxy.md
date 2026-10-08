# The macOS system proxy: steps for the user

Date: 2026-10-08. Plan milestone 10, item 3.

## Question

Does `ncp` use the manual HTTP and HTTPS proxy of the macOS system
settings when no proxy variable is set (design C5, requirement N2)? Does
`-v` show the route and its source?

## Why automation cannot test it

The system settings are global to the Mac. A test that changes them
changes the network of each program of the user, and it needs an
administrator password. Thus a person does these steps on a real Mac.

## Method

You need: a Mac, `ncp` from this repository, and a proxy that logs each
request. The steps use `mitmdump` as an example; another logging proxy
also works.

1. Build `ncp`:

   ```
   cargo build -p ncp --release
   ```

2. In a new terminal, start the proxy on port 8080 and keep it open:

   ```
   mitmdump --listen-port 8080
   ```

3. Configure one HTTP server in `~/.config/ncp/mcp.json`, for example a
   public MCP server that needs no login.

4. Open System Settings > Network > (your network) > Details > Proxies.
   Set "Web proxy (HTTP)" and "Secure web proxy (HTTPS)" to `127.0.0.1`,
   port `8080`. Click OK.

5. In a terminal with no proxy variables, run:

   ```
   env -u HTTP_PROXY -u HTTPS_PROXY -u ALL_PROXY -u http_proxy \
     -u https_proxy -u all_proxy -u NO_PROXY -u no_proxy \
     ./target/release/ncp -v tool list SERVER
   ```

6. Turn the two proxies off again in System Settings.

## Expected result

- In step 5, `-v` shows `route to URL: proxy http://127.0.0.1:8080/ from
  the system settings`.
- The proxy log shows a `CONNECT` request for the host of the server.
- With `HTTPS_PROXY` set in step 5, `-v` names `HTTPS_PROXY` as the
  source, and the system settings have no effect.

## Limits

`ncp` reads only the manual HTTP and HTTPS proxy. It does not read the
automatic proxy configuration (PAC) or the bypass list of the system
settings (design C5). `NO_PROXY` is the only exception list.

## Result

Not run yet. The user records the result here.
