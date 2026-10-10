# smolvm as the runtime of the Linux test harness

Status: research, 2026-10-10. Decision: keep the Docker harness. Do not
add smolvm now.

Method: hands-on runs on this Mac (Apple M4, 10 cores, OrbStack Docker
29.4.0, smolvm 1.25.4 from the Homebrew tap of the user). The smolvm
machines used the images of the current harness (`docker save`), so both
runtimes ran the same toolchain. Both had 10 vCPUs and about 12 GB of
memory. Source reads: the smolvm README, `docs/limitations.md`,
`docs/smolfile.md`, `scripts/rosetta/rosetta-wrapper.c`, and the
`smolvm --help` reference. All eval machines and volumes were removed
after the runs.

## 1. Summary

| Item | Docker (OrbStack) | smolvm | Result |
|---|---|---|---|
| Start of a trivial command (warm) | 0.55 s | 1.06 s | Not important for runs of 20 s or more |
| Full cold build, arm64 (`cargo test --no-run`) | 30.6 s | 28.0 s | Equal |
| Cold build with proc macros kept, 2 runs | 26.3 s, 23.6 s | 20.8 s, 22.0 s | smolvm is a little faster |
| Test run, arm64 | 20.0 s | 18.9 s | Equal; all tests pass in both |
| Incremental build after a host edit | 1.1 s | 0.75 s | Both see the host change |
| Secret Service tests | pass | pass, no script change | Equal |
| Build output on a host mount (virtiofs) | not used | 56 s full cold | 2 times slower: the cache must be on the VM disk |
| amd64 tests (Rosetta) | all pass | 1 test fails each time | smolvm defect, see section 3 |
| Network | all or nothing (`--network none`) | off, allow list of host names, or localhost only | smolvm is better |
| Image build | Dockerfile | no build; needs `docker save` or a Smolfile with `init` commands | smolvm still needs Docker, or a second image recipe |
| Persistent caches | named volumes | no named volumes; a persistent machine or a branch | smolvm needs more steps |
| Location of data | an external disk (moved by the user) | `~/Library/Caches/smolvm` on the root disk, fixed path | smolvm brings back the risk of a full root disk |

## 2. What smolvm does better

- Egress allow list. With `--allow-host-pattern index.crates.io` and
  `--allow-host-pattern static.crates.io`, `cargo fetch` worked. A name
  lookup of `github.com` and `deb.debian.org` failed. A connection to a
  direct IP address (`1.1.1.1:443`) also failed. Thus the fetch step can
  run with network access to crates.io only. The Docker harness gives the
  fetch step full network access.
- Safe network changes. `machine update` refuses to remove the last allowed
  host, because that change gives access to all hosts. The tool asks for an
  explicit `--net` or `--no-net`.
- Each machine has its own kernel. The harness does not need this: the
  tests run code from this repository and from pinned crates only.
- Branches. `machine branch` made a copy-on-write child in 0.44 s, and
  `machine delete` removed it in 0.53 s. Writes in the child did not go to
  the base machine. This can give a clean state for each run, with warm
  caches from the base.

## 3. Defect: process signals under Rosetta

The test
`copy_check_of_a_slow_native_store_stops_at_its_limit_and_leaves_no_program`
failed 5 times in 5 runs in an amd64 smolvm machine. It passed in the amd64
Docker harness and in the arm64 smolvm machine.

Cause: smolvm registers `/usr/bin/rosetta-wrapper` as the `binfmt_misc`
interpreter for x86_64 programs. The wrapper forks the real program, traces
it with ptrace to answer a Rosetta check, and stays as its parent. Thus the
process ID that a parent gets is the ID of the wrapper. A signal to that ID
stops the wrapper only. The real program continues as an orphan of PID 1.

Proof, in the same guest:

```
sleep 30 & kill -9 $!
# before: 10180 /usr/bin/rosetta-wrapper /usr/bin/sleep 30
#         10182 (child of 10180) /usr/bin/sleep 30
# after:  10182 (child of 1) /usr/bin/sleep 30   <- still runs
```

In the Docker harness, the same commands stop the program.

`ncp` stops stdio servers and credential store programs with signals, and
the tests check that no program stays. Under smolvm amd64, these tests show
false failures. A real defect could also hide behind the wrapper. Upstream
issue 894 reports a different failure of the same wrapper.

## 4. Costs for this project

- Two tools in place of one. smolvm does not build images. The Dockerfile
  stays, or a Smolfile repeats the image recipe with `init` commands.
- Disk. The VM disks are in `~/Library/Caches/smolvm`, on the root disk.
  The path comes from `dirs::data_local_dir` and has no setting. The root
  disk was full on 2026-10-08 because of container data. A file delete in
  the guest gives the space back to the host (3.4 GB to 1.6 GB in a test).
- No named volumes. The registry and the build output must stay in a
  persistent machine. A persistent machine keeps state between runs, for
  example the `/etc/passwd` line of `secret-service.sh` and orphan
  processes. `docker run --rm` starts each run clean.
- Network changes need a stop and a start of the machine.
- Release rate and project size. 38 releases in the last 30 days, and one
  author made about 90% of the commits. A harness needs a pinned version
  and a review of each update.
- Policy. The project rules ask the user before a new network destination
  and before code from a new source. smolvm adds both: the tap or the
  release download, and the guest agent image.

## 5. Recommendation

Keep the Docker harness. smolvm gives no speed gain that matters here, and
it breaks process signals for amd64 programs. That part is central to the
stdio and credential store tests of `ncp`.

Take one idea from smolvm: limit the network of the fetch step to
crates.io. The Docker harness can do this without smolvm, for example with
an egress proxy that allows only crates.io hosts. That change is optional.

Check smolvm again if one of these becomes true:

- The amd64 wrapper sends signals to the real program, or smolvm runs
  x86_64 programs with no wrapper parent.
- The VM data can be on a disk that the user selects.
- The harness must run code that is not trusted, so that a separate kernel
  is necessary.
