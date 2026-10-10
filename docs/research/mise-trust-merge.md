# Study: how mise merges a trusted project file with the global file

Date: 2026-10-11. Input for decision B of
[project-config.md](../design/project-config.md). It adds to part 3 of
[project-trust-prior-art.md](project-trust-prior-art.md).

**Method.** One research agent read the source and the documentation of
mise at commit `d40eb9af9bd05e51a023838eae026070ea779d5b`
(github.com/jdx/mise). It did not build or run the code. Paths are
relative to the root of the repository. Each fact has a mark: V (the agent
read the source or the document) or I (inferred, often from a search that
found nothing). The facts have no recommendations.

## 1. Order and merge

| Item | Fact | Evidence | Mark |
|---|---|---|---|
| File order | Lowest first: system (`/etc/mise`), global (`~/.config/mise/config.toml`), then each folder from `/` down to the current folder. In one folder, `mise.local.toml` wins over `mise.toml`. | `docs/configuration.md:84-93`; `src/config/mod.rs:2888`, `3192` | V |
| `[tools]` | For the same tool, the nearest file replaces the whole entry, with its options. Other tools come from the other files. | `src/toolset/mod.rs:117-127`; `src/toolset/builder.rs:152`; `docs/configuration.md:149-161` | V |
| `[env]` | The nearest file wins for each variable. | `src/config/mod.rs:1646`; `src/config/env_directive/mod.rs:557`, `627` | V |
| `[tasks]` | The nearest file wins for each task name. | `src/config/mod.rs:1275-1281`, `3297-3302`; `docs/configuration.md:225` | V |
| `[settings]` | The nearest file wins for each key. The order comes from the call order of the builder. | `src/config/settings.rs:1370`, `1403`, `1636` | I |

## 2. A project file that is not trusted

| Case | Fact | Evidence | Mark |
|---|---|---|---|
| A command with a TTY | mise asks "config files in <folder> are not trusted. Trust them?". The answers are Yes, No and All. The default is No. | `src/config/config_file/mod.rs:633-636`; `src/ui/prompt.rs:213-219` | V |
| A command with no TTY | The load of the configuration fails. mise does not fall back to the global file. The error names the folder and `mise trust`. | `src/ui/prompt.rs:47-58`, `202-203`; `src/config/mod.rs:3868-3870`; `src/errors.rs:35-40` | V |
| The shell hook (`hook-env`) | mise skips the file and gives a warning. | `src/cli/hook_env.rs:60`, `228-241` | V |
| Automatic trust | `MISE_YES`, `--yes` and the `CI` variable trust each file. `run`, `exec`, `install` and `watch` trust it implicitly. Paranoid mode turns all of these off. | `src/config/config_file/mod.rs:605-615`, `628-641`, `720-721` | V |
| Content with no trust | A file with only plain tool versions loads with no trust. The check is for the whole file. | `src/config/config_file/mise_toml.rs:3318-3357` | V |

## 3. After the trust

| Item | Fact | Evidence | Mark |
|---|---|---|---|
| Message about a replaced value | None. A search for an override warning found nothing. | search of `src/config` | I |
| What the question shows | The folder only. It shows no keys, values or content. | `src/config/config_file/mod.rs:633-636` | V |
| A global-only key in a project file | mise ignores the key and gives a warning. Examples: `paranoid`, `yes`, `trusted_config_paths`, the GitHub and GitLab settings. | `src/config/settings.rs:497-516`; `settings.toml` (`global_only = true`) | V |

## 4. Records

| Item | Fact | Evidence | Mark |
|---|---|---|---|
| Place | `$XDG_STATE_HOME/mise/trusted-configs/` and `ignored-configs/`. | `crates/mise-util/src/dirs.rs:27-28` | V |
| Key | A hash of the canonical path. Paranoid mode makes the key one file, not a folder, and adds the SHA-256 of the content. | `src/config/config_file/mod.rs:530-535`, `958-972`, `1011-1062` | V |
| Refusal | A No writes an ignore record for the folder. mise does not ask again until `mise trust`, which removes the record. No answer (no TTY, or the shell hook) writes nothing. | `src/config/config_file/mod.rs:627`, `645-648`, `959` | V |

## 5. Mismatch of the documents and the code

`docs/security.md:51-53` says that `[settings]` needs trust. The code
loads the `[settings]` of a project file with no trust check; only safe
mode removes them (`src/config/settings.rs:1003-1008`, `1436-1438`).
Mark: I. Not checked by a run.
