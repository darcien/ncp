# A custom CA file: add to the system roots, or replace them

Date: 2026-10-10. Requirement N4, design B50.

## Question

When `SSL_CERT_FILE` is set, does `ncp` trust the roots in the file in
addition to the system roots, or in place of them? The rule must be the
same on macOS and Linux.

## Facts about `ncp`

- `ncp` uses reqwest 0.13.5 with rustls and rustls-platform-verifier 0.7.1.
- Before this decision, `ncp` gave the file to reqwest as extra roots
  (`tls_certs_merge`).
- On macOS, the verifier uses the Keychain, so the extra roots are added.
- On Linux, the verifier calls `rustls_native_certs::load_native_certs()`
  (rustls-platform-verifier 0.7.1, `src/verification/others.rs`). In
  rustls-native-certs 0.8.4, this function loads only the files that the
  variables name when `SSL_CERT_FILE` or `SSL_CERT_DIR` is set. The crate
  documentation says: "certificates are only loaded from the locations
  specified via environment variables and not the platform-native
  certificate store". Thus the file replaces the system roots on Linux.
- An empty `SSL_CERT_FILE` counts as set in rustls-native-certs, so on
  Linux it loads no root.
- To add on Linux, `ncp` must remove the variable from its own environment
  (`unsafe` in Rust 2024, and the workspace forbids `unsafe`), or find and
  load the system roots itself.

## How uv does it

uv 0.12.24 (tag of 2026-10-08). The docs and the client code were read at
this tag.

- When `SSL_CERT_FILE` or `SSL_CERT_DIR` has a non-empty value, uv trusts
  only those roots, on each OS, with one code path:
  `tls_certs_only(custom_certs)` in `crates/uv-client/src/base_client.rs`.
- The docs (`docs/concepts/authentication/certificates.md`): "When set to
  non-empty values, these environment variables **override** the default
  certificate source entirely — only the provided certificates will be
  trusted."
- uv 0.11.0 (PR #18550) rejected a change that merged the file with the
  default roots. It called the merge an unintentional breaking change.
- uv 0.12.0 (PR #20741): a file that is missing or has no certificate no
  longer falls back to the default roots. The reason in the changelog: the
  fallback could allow HTTPS connections that the override was meant to
  reject.
- uv ignores an empty value.
- Issue #16474: a user put only the proxy root in the file, and public hosts
  failed. The maintainers called this expected behavior: the file must hold
  each root that the user needs.
- uv gives no hint that names the variable when a connection fails with an
  unknown issuer.

## Decision

- `ncp` trusts only the roots in the file, on macOS and on Linux, as uv
  does.
- A file that `ncp` cannot read, or that has no certificate, gives an
  error. This fails closed, as uv does.
- An empty value also gives an error. `ncp` cannot ignore it as uv does,
  because the Linux system store then loads no root.
- When a connection fails with an unknown issuer and the variable is set,
  the hint says that the file replaces the system roots. This is the common
  error of issue #16474.
