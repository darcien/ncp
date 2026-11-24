import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
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

  // The layout of pnpm: the suite is a link, and yaml is only beside the
  // target of the link.
  const store = join(root, "node_modules", ".pnpm", "conformance", "node_modules");
  const suite = join(store, "@modelcontextprotocol", "conformance");
  const yaml = join(store, "yaml");
  const results = join(root, "results", "requirements-test");
  mkdirSync(join(suite, "requirements"), { recursive: true });
  mkdirSync(yaml, { recursive: true });
  mkdirSync(results, { recursive: true });
  mkdirSync(join(root, "node_modules", "@modelcontextprotocol"));
  symlinkSync(suite, join(root, "node_modules", "@modelcontextprotocol", "conformance"));
  copyFileSync(join(source, "report.mjs"), join(root, "report.mjs"));

  // JSON is valid YAML. This local parser keeps the fixture free of downloads.
  writeFileSync(join(yaml, "index.js"), "exports.parse = JSON.parse;\n");
  writeFileSync(join(suite, "package.json"), JSON.stringify({ version: "test" }));
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
  assert.equal(report.suite.version, "test");
  const scenarios = report.runs[0].scenarios;
  for (const [name, checks, result] of cases) {
    const scenario = scenarios.find((entry) => entry.scenario === name);
    assert.equal(scenario.result, result, name);
    assert.deepEqual(scenario.checks, (checks ?? []).filter((check) => check.id !== "outgoing-request"), name);
  }
});
