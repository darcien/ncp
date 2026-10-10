# Study: secure defaults of pnpm for credentials and approvals

Date: 2026-10-10. Input for [secret-binding.md](../design/secret-binding.md).

**Method.** The main session read the pnpm documentation on 2026-10-10:
<https://pnpm.io/npmrc>, <https://pnpm.io/settings/build>,
<https://pnpm.io/cli/approve-builds>,
<https://pnpm.io/settings/dependency-resolution>. Nothing was installed.
The user chose pnpm, not npm, as the reference for secure defaults: npm
expands `${VAR}` in a project `.npmrc` and runs install scripts by default.

## 1. Credentials

| Fact | Version | Quote or summary |
|---|---|---|
| `_auth` is keyed by registry URL | v11.10.0 | "The value is keyed by registry URL, so each secret is explicitly bound to the host that may receive it." Read only from the global `config.yaml` or the `pnpm_config__auth` variable; ignored in a project `pnpm-workspace.yaml` or `.npmrc`. |
| No `${...}` expansion in a project `.npmrc` | v11.5.3 | "The project `.npmrc` is checked out together with the repository"; expansion there "would allow a malicious repository to exfiltrate secrets from your environment". A registry URL, URL-scoped key or credential value with a `${...}` placeholder "is ignored, and pnpm prints a warning". |
| `${NAME}` expands in user-level auth files only | — | `<pnpm config>/auth.ini` and the user `.npmrc`. |
| `tokenHelper` | — | Absolute path, no arguments, only in the user `.npmrc`, not from the environment. |
| Scope-specific tokens | v11.7.0 | Before: "two scopes sharing a registry had to share a token". |

## 2. Approvals

| Fact | Version | Quote or summary |
|---|---|---|
| `strictDepBuilds`, default true | v10.3.0 | The install "will exit with a non-zero exit code if any dependencies have unreviewed build scripts" (`ERR_PNPM_IGNORED_BUILDS`). |
| `allowBuilds` | v10.26.0 | A map of package to `true` or `false` in `pnpm-workspace.yaml`. "Packages not listed in `allowBuilds` are disallowed by default and are treated as unreviewed." |
| `pnpm approve-builds` | — | Interactive, or package names as arguments; `!name` denies. The decision goes to the file. |
| `dangerouslyAllowAllBuilds`, default false | v10.9.0 | The escape path has a name that tells the risk. |

## 3. Supply chain (context)

- `minimumReleaseAge`: default 1440 minutes since v11 (0 before).
- `trustPolicy: no-downgrade` (opt-in, v10.21.0): fail when the trust level
  of a package goes down.
- `blockExoticSubdeps`: default true since v10.26.0.

## 4. Consequences for `ncp`

- Key a stored secret by the destination that may receive it, as `_auth`
  does.
- Do not expand `${NAME}` from the environment in a credential position of
  a project file. The user file can expand it.
- A `${...}` in a destination (registry URL) is not accepted.
- Fail closed with a non-zero exit, and give one command that records the
  decision.
