---
state: committed
supersedes: []
---

# 0002: Conformance result on the site

## Decision

The site shows one result from the official MCP conformance suite. The public
`conformance/conformance.json` is its only long-term store. No result goes into
git. The page names the tested commit and date before it shows the results.
It does not claim that the result applies to the current `main` commit.

The home page remains a short list of links. The OAuth client metadata keeps
its fixed URL. The site has no result history or release gate.

## Runs

A push to `main` starts conformance when it changes `crates/**`, `Cargo.toml`,
`Cargo.lock`, `rust-toolchain.toml`, `mise.toml` or
`harness/conformance/**`. A person can also start a run. A site-only change
does not start conformance. A release tag needs no separate run.

CI uses `mise run conformance:install` and `mise run conformance`, as local
runs do. Rust uses `rust-toolchain.toml`. The existing runner writes one JSON
file from the two requirement sets. Its other legs remain checks. The report
includes failures, even when the suite job fails. A build failure can leave
no report. The suite job has no Pages permission.

## Publication

The Pages workflow deploys on a change to `site/**` or its workflow file. It
does not wait for conformance. Completion of a conformance workflow on `main`
also starts a deploy, even when that workflow fails. Each deploy fetches the
latest `main` before it assembles the whole site. This prevents a slow suite
run from publishing old OAuth metadata.

Before a deploy, the job tries to read the published JSON. For a conformance
completion, it also tries to read that run's artifact. It copies only the
expected JSON file. A new file must have schema `1`, complete run data and a
tested commit equal to the source workflow run's commit. Only a workflow run
from this repository on `main` can supply it.

If both files exist, the job uses the new one only when its commit is the
same as, or a descendant of, the public result's commit on `main`. The job
reads git history for this comparison. Otherwise, it keeps the public file.
If one file exists, it uses that file. If neither exists, it deploys the site
without a result. The page then says that no result is available. No result
failure holds back the OAuth metadata.

Deploy jobs share one concurrency group. GitHub can replace a pending deploy.
Each deploy reads the current site files, so a replaced site deploy does not
lose a change to the OAuth metadata. A replaced result deploy can lose its
result.

## Limits

Pages is not a durable data store. A failed read can remove the only public
result on the next deploy. A stale Pages response can cause an older result
to replace a newer one, despite the commit comparison. Workflow artifacts
expire. The page always names the tested commit. Strict result ordering or
durable retention requires another store. This design accepts those limits
to keep OAuth metadata independent of conformance.
