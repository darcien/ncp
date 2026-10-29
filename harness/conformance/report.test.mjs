import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const source = dirname(fileURLToPath(import.meta.url));

test("reports incomplete checks as failed without changing recorded checks", (t) => {
  const root = mkdtempSync(join(tmpdir(), "ncp-report-test-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));

  const suite = join(root, "node_modules", "@modelcontextprotocol", "conformance");
  const yaml = join(root, "node_modules", "yaml");
  const results = join(root, "results", "requirements-test");
  mkdirSync(join(suite, "requirements"), { recursive: true });
  mkdirSync(yaml, { recursive: true });
  mkdirSync(results, { recursive: true });
  copyFileSync(join(source, "report.mjs"), join(root, "report.mjs"));

  // JSON is valid YAML. This local parser keeps the fixture free of downloads.
  writeFileSync(join(yaml, "package.json"), '{"type":"module","exports":"./index.mjs"}');
  writeFileSync(join(yaml, "index.mjs"), "export const parse = JSON.parse;\n");
  writeFileSync(join(root, "package-lock.json"), JSON.stringify({
    packages: { "node_modules/@modelcontextprotocol/conformance": { version: "test" } },
  }));
  writeFileSync(join(root, "expected-failures.yaml"), JSON.stringify({ client: [] }));

  const cases = [
    ["empty", [], "failed"],
    ["info-only", [{ id: "note", status: "INFO" }], "failed"],
    ["message-only", [{ id: "outgoing-request", status: "INFO" }], "failed"],
    ["success", [{ id: "complete", status: "SUCCESS" }], "passed"],
    ["warning", [{ id: "complete", status: "SUCCESS" }, { id: "warn", status: "WARNING" }], "failed"],
    ["failure", [{ id: "complete", status: "SUCCESS" }, { id: "fail", status: "FAILURE" }], "failed"],
    ["missing", null, "failed"],
  ];
  writeFileSync(join(suite, "requirements", "test.yaml"), JSON.stringify({
    client: cases.map(([name]) => name),
    not_scored: [],
  }));
  for (const [name, checks] of cases) {
    const dir = join(results, `${name}-2026-10-21T13-12-38-339Z`);
    mkdirSync(dir);
    if (checks !== null) writeFileSync(join(dir, "checks.json"), JSON.stringify(checks));
  }

  const process = spawnSync(globalThis.process.execPath, [
    join(root, "report.mjs"), "1.0", "abc", "test-target", "test",
  ], { encoding: "utf8" });
  assert.equal(process.status, 0, process.stderr);
  const report = JSON.parse(readFileSync(join(root, "results", "conformance.json"), "utf8"));
  const scenarios = report.runs[0].scenarios;
  for (const [name, checks, result] of cases) {
    const scenario = scenarios.find((entry) => entry.scenario === name);
    assert.equal(scenario.result, result, name);
    assert.deepEqual(scenario.checks, (checks ?? []).filter((check) => check.id !== "outgoing-request"), name);
  }
});
