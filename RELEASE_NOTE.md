This release corrects RUSTSEC-2026-0457. A hostile server can no longer send
the OAuth requests of `ncp` to a service in your network or on your computer.
If the authorization server of a server is in a private network, set
`oauth.allowPrivateAddresses` to `true` in the entry of the server.
