// Writes results/conformance.json from the requirements runs of run.sh.
// run.sh gives the arguments: VERSION, COMMIT and TARGET of the program
// that the suite tested, then the revisions that it ran. A revision with no
// results directory stops the writer, so that the file does not look complete.
//
// The suite judges each run. This writer derives scenario results from checks.
// At each suite update, confirm these facts and the yaml package dependency:
// - A timeout can leave no SUCCESS check, even when checks.json exists.
// - A requirements run compares the baseline only with the client
//   scenarios of the requirement set. A failure of a not_scored scenario
//   does not make the run fail.
// - The suite makes the directory <scenario>-<time> before the scenario
//   starts. A scenario that stops with an exception has no checks.json.
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { parse } from "yaml";

const here = import.meta.dirname;
const results = join(here, "results");
const suiteName = "@modelcontextprotocol/conformance";
const suiteDir = join(here, "node_modules", suiteName);

// The time suffix of a scenario directory, for example -2026-10-21T13-12-38-339Z.
const timeSuffix = /-\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}-\d{3}Z$/;
// These checks log one message each. They are about two thirds of all
// checks and tell nothing about conformance.
const isMessageLog = (check) =>
  check.status === "INFO" && /^(incoming|outgoing)-/.test(check.id);

// A JSON error of Node does not name the file.
function readJson(path) {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    throw new Error(`report.mjs: cannot read ${path}`, { cause: error });
  }
}

const readYaml = (path) => parse(readFileSync(path, "utf8"));

// Maps each client scenario of the requirement set to its requirement:
// required, or the reason under not_scored.
function requirements(revision) {
  const set = readYaml(join(suiteDir, "requirements", `${revision}.yaml`));
  const notScored = set.not_scored
    .filter((entry) => entry.leg === "client")
    .map((entry) => [entry.scenario, entry.reason]);
  const required = set.client.map((scenario) => [scenario, "required"]);
  return new Map([...notScored, ...required]);
}

// The scenario directories of a run, as [name, path]. An auth/ scenario has a
// directory level for auth. The sort of readdirSync output does not use the
// locale, so the output is the same on each machine.
function scenarioDirs(dir, prefix = "") {
  return readdirSync(dir)
    .sort()
    .flatMap((name) =>
      timeSuffix.test(name)
        ? [[prefix + name.replace(timeSuffix, ""), join(dir, name)]]
        : scenarioDirs(join(dir, name), `${prefix}${name}/`),
    );
}

function scenario([name, path], requirementOf, baseline) {
  // Without this stop, JSON leaves out a requirement that is undefined.
  const requirement = requirementOf.get(name);
  if (!requirement) {
    throw new Error(`report.mjs: the requirement set has no scenario ${name}`);
  }
  const checksFile = join(path, "checks.json");
  const checks = existsSync(checksFile)
    ? readJson(checksFile).filter((check) => !isMessageLog(check))
    : null;
  const failed =
    checks === null ||
    !checks.some((check) => check.status === "SUCCESS") ||
    checks.some((check) => check.status === "FAILURE" || check.status === "WARNING");
  return {
    scenario: name,
    requirement,
    result: failed ? "failed" : "passed",
    knownFailures: baseline.filter((line) => line === name || line.startsWith(`${name}:`)),
    checks: checks ?? [],
  };
}

function run(revision, baseline) {
  const requirementOf = requirements(revision);
  const scenarios = scenarioDirs(join(results, `requirements-${revision}`)).map((entry) =>
    scenario(entry, requirementOf, baseline),
  );
  // Without this stop, a required scenario that the suite did not run
  // is not in the file, and the run looks complete.
  const missing = [...requirementOf]
    .filter(([, requirement]) => requirement === "required")
    .find(([name]) => !scenarios.some((s) => s.scenario === name));
  if (missing) {
    throw new Error(`report.mjs: the run of ${revision} has no directory for ${missing[0]}`);
  }
  return { revision, scenarios };
}

const [version, commit, target, ...revisions] = process.argv.slice(2);
const lock = readJson(join(here, "package-lock.json"));
const baseline = readYaml(join(here, "expected-failures.yaml")).client;
const report = {
  schema: 1,
  ncp: { version, commit, target },
  suite: { version: lock.packages[`node_modules/${suiteName}`].version },
  date: new Date().toISOString(),
  runs: revisions.map((revision) => run(revision, baseline)),
};
const output = join(results, "conformance.json");
writeFileSync(output, `${JSON.stringify(report, null, 2)}\n`);
console.log(`report.mjs: wrote ${output}`);
