const repo = "https://github.com/darcien/ncp";

// The strings come from the suite, so each one is escaped. A value that is not a string shows as JSON.
const escape = (value) =>
  (typeof value === "string" ? value : JSON.stringify(value) ?? "").replace(
    /[&<>"']/g,
    (char) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[char],
  );

// Only a web link becomes a link, so that a file cannot add a script URL.
const isWeb = (url) => typeof url === "string" && /^https?:\/\//.test(url);
const link = (url, text) => (isWeb(url) ? `<a href="${escape(url)}">${escape(text)}</a>` : escape(text));

// Only a full commit hash goes into a URL, so that a file cannot change
// the path of a link to the repository.
const isCommit = (value) => typeof value === "string" && /^[0-9a-f]{40}$/.test(value);

// An id holds only letters, digits and hyphens, so that a file cannot add
// markup through it.
const idOf = (name) => `detail-${String(name).replace(/[^A-Za-z0-9]+/g, "-")}`;

const isRequired = (scenario) => scenario.requirement === "required";
const isFailed = (scenario) => scenario?.result === "failed";
const isBad = (check) => check.status === "FAILURE" || check.status === "WARNING";

// One mark and one color for each state, in each place of the page. The
// group of a scenario, not its color, tells if it counts.
const marks = { passed: "✓", failed: "✗", warning: "!", none: "–" };
const result = (state) => `<span class="${state}">${marks[state] ?? ""}</span>`;
const notInRevision = `<td class="none">${marks.none}</td>`;
const countCell = (passed, total) => {
  const state = passed === total ? "passed" : "failed";
  return `<td class="${state}">${marks[state]}&nbsp;${passed}/${total}</td>`;
};

// A check passes or fails as a scenario does.
const checkState = (status) =>
  ({ SUCCESS: "passed", FAILURE: "failed", WARNING: "warning", INFO: "info", SKIPPED: "skipped" })[status] ?? escape(status);

// The spec area of a scenario is the spec page that its checks cite most,
// because a reader thinks in spec areas and not in suite names.
const specPath = (url) => {
  try {
    const rest = /\/specification\/[^/]+\/?(.*)$/.exec(new URL(url).pathname)?.[1];
    return rest === undefined ? null : rest.replace(/\/$/, "") || "basic";
  } catch {
    return null;
  }
};
const areaOf = (scenario) => {
  const tally = new Map();
  for (const ref of scenario.checks.flatMap((check) => check.specReferences ?? [])) {
    const path = isWeb(ref.url) ? specPath(ref.url) : null;
    if (path) tally.set(path, (tally.get(path) ?? 0) + 1);
  }
  return [...tally].sort((a, b) => b[1] - a[1])[0]?.[0] ?? "";
};
const areaNames = {
  basic: "Base protocol",
  "basic/index": "Base protocol",
  "basic/authorization": "Authorization",
  "basic/transports": "Transports",
  "basic/lifecycle": "Lifecycle",
  "basic/utilities/mrtr": "Utilities: mrtr",
  "server/tools": "Tools",
  "server/resources": "Resources",
  "server/prompts": "Prompts",
  "client/elicitation": "Elicitation",
  "client/sampling": "Sampling",
  "client/roots": "Roots",
};
const areaTitle = (path) => (!path ? "Other" : areaNames[path] ?? `<code>${escape(path)}</code>`);

// Several references share one ID, so the link text also names the spec
// page and the section.
const reference = (ref) => {
  try {
    const place = new URL(ref.url);
    const part = `${place.pathname.split("/").filter(Boolean).slice(-2).join("/")}${place.hash}`;
    return link(ref.url, `${ref.id} (${part})`);
  } catch {
    return escape(ref.id);
  }
};

// Some failed checks have no error text. The description then tells what
// the client did not do.
const why = (check) => check.errorMessage ?? check.details?.message;
const cause = (check) =>
  check ? `${escape(check.name)}: ${escape(why(check) ?? check.description ?? "")}` : "The scenario stopped before its checks.";

const plural = (count, word) => `${count} ${word}${count === 1 ? "" : "s"}`;
const revisionNames = (revisions) => {
  const names = revisions.map((revision) => `MCP ${escape(revision)}`);
  return names.length > 1 ? `${names.slice(0, -1).join(", ")} and ${names.at(-1)}` : names.join("");
};

// One row for each scenario name, with its scenario in each revision, or
// null when the revision does not have it.
const rowsOf = (runs) => {
  const rows = new Map();
  runs.forEach((run, column) => {
    for (const scenario of run.scenarios) {
      const row = rows.get(scenario.scenario) ?? { name: scenario.scenario, cells: runs.map(() => null) };
      row.cells[column] = scenario;
      rows.set(scenario.scenario, row);
    }
  });
  return [...rows.values()].map((row) => {
    const present = row.cells.filter(Boolean);
    const required = present.find(isRequired);
    return { ...row, requirement: required ? "required" : present[0].requirement, area: areaOf(required ?? present[0]) };
  });
};

// Failed checks first. The other checks are behind a count, because they
// are most of the checks and tell nothing new.
const checksTable = (checks) => {
  if (!checks.length) return "<p>The scenario stopped before its checks.</p>";
  const head = `<thead><tr><th scope="col">Result</th><th scope="col">Check</th><th scope="col">Spec</th></tr></thead>`;
  const body = (list) =>
    list
      .map((check) => {
        const state = checkState(check.status);
        return `<tr><td class="${state}">${marks[state] ?? ""} ${state}</td>
<td><div class="stack-sm"><span>${escape(check.name)}</span><small>${escape(check.description ?? "")}</small>${why(check) ? `<span>${escape(why(check))}</span>` : ""}</div></td>
<td><div class="stack-sm">${(check.specReferences ?? []).map(reference).join("")}</div></td></tr>`;
      })
      .join("");
  const bad = checks.filter(isBad);
  const rest = checks.filter((check) => !isBad(check));
  const table = (list) => `<table class="checks">${head}<tbody>${body(list)}</tbody></table>`;
  const restLabel = bad.length ? plural(rest.length, "other check") : plural(rest.length, "check");
  return `${bad.length ? table(bad) : ""}${rest.length ? `<details><summary>${restLabel}</summary>${table(rest)}</details>` : ""}`;
};

// One detail row for each failed scenario. When the failed checks are the
// same in each revision, the row shows them one time.
const detailRow = (row, runs) => {
  const failed = row.cells
    .map((s, column) => (isFailed(s) ? { s, revisions: [runs[column].revision] } : null))
    .filter(Boolean);
  if (!failed.length) return "";
  const signature = ({ s }) => JSON.stringify(s.checks.filter(isBad).map((c) => [c.id, c.status, why(c)]));
  const parts = failed.every((f) => signature(f) === signature(failed[0]))
    ? [{ s: failed[0].s, revisions: failed.flatMap((f) => f.revisions) }]
    : failed;
  return `<tr class="detail" id="${idOf(row.name)}" hidden><td colspan="${runs.length + 1}"><div class="stack">
${parts
  .map((part) => `<h3><code>${escape(row.name)}</code> in ${revisionNames(part.revisions)}</h3>${checksTable(part.s.checks)}`)
  .join("")}</div></td></tr>`;
};

const cell = (scenario) => scenario
  ? `<td>${result(isFailed(scenario) ? "failed" : "passed")}${isRequired(scenario) ? "" : `<br><small>${escape(scenario.requirement)}</small>`}</td>`
  : notInRevision;

// The name of a failed scenario opens its checks, as the name of an area
// opens its scenarios. The checks of a passed scenario tell nothing new.
const scenarioName = (row) => {
  const name = `<code>${escape(row.name)}</code>`;
  return row.cells.some(isFailed)
    ? `<button type="button" aria-expanded="false" aria-controls="${idOf(row.name)}">${name}</button>`
    : name;
};

const tally = (rows, column, requiredOnly) => {
  const present = rows.map((row) => row.cells[column]).filter((s) => s && (!requiredOnly || isRequired(s)));
  if (!present.length) return notInRevision;
  return countCell(present.filter((s) => !isFailed(s)).length, present.length);
};

// A group is open only when a required scenario in it failed, so that the
// table shows the answer first and the detail on demand.
const group = (title, rows, runs, requiredOnly = true) => {
  const open = rows.some((row) => row.cells.some((s) => s && isRequired(s) && isFailed(s)));
  return `<tbody${open ? "" : " data-collapsed"}>
<tr class="area"><th scope="rowgroup"><button type="button" aria-expanded="${open}">${title}</button></th>
 ${runs.map((run, column) => tally(rows, column, requiredOnly)).join("")}</tr>
${rows
  .map(
    (row) => `<tr class="scenario"><th scope="row">${scenarioName(row)}${
      row.requirement === "required" ? "" : ` <small>${escape(row.requirement)}</small>`
    }</th>${row.cells.map(cell).join("")}</tr>
${detailRow(row, runs)}`,
  )
  .join("")}
</tbody>`;
};

// The score of each revision is a header row, as in a compatibility table,
// so that the answer and the table are one view.
const matrix = (runs, rows) => {
  const required = rows.filter((row) => row.requirement === "required");
  const other = rows.filter((row) => row.requirement !== "required");
  // The largest area first, and "Other" last.
  const size = (area) => (area ? required.filter((row) => row.area === area).length : -1);
  const areas = [...new Set(required.map((row) => row.area))].sort((a, b) => size(b) - size(a));
  const score = (run) => {
    const list = run.scenarios.filter(isRequired);
    return countCell(list.filter((s) => !isFailed(s)).length, list.length);
  };
  return `<table class="matrix">
<thead>
<tr><th scope="col">MCP revision</th>${runs.map((run) => `<th scope="col">${escape(run.revision)}</th>`).join("")}</tr>
<tr><th scope="row">Required scenarios that passed</th>${runs.map(score).join("")}</tr>
</thead>
${areas.map((area) => group(areaTitle(area), required.filter((row) => row.area === area), runs)).join("")}
 ${other.length ? group("Not required", other, runs, false) : ""}
</table>`;
};

// Each failed required scenario shows one time with its first failed
// check, so that a reader sees the cause with no click.
const failures = (runs, rows, known) => {
  const failedIn = (row) => row.cells.map((s, column) => (s && isRequired(s) && isFailed(s) ? runs[column].revision : null)).filter(Boolean);
  const gaps = rows.filter((row) => row.cells.some((s) => s && isRequired(s) && isFailed(s)));
  const item = (row) =>
    `<li><span><code>${escape(row.name)}</code> <small>in ${revisionNames(failedIn(row))}</small></span><span>${cause(row.cells.find((s) => s && isRequired(s) && isFailed(s)).checks.find(isBad))}</span></li>`;
  return `<section class="stack"><h2>Failed required scenarios</h2>
${gaps.length ? `<p>The ${known} gives the reason for each known failure.</p><ul class="causes">${gaps.map(item).join("")}</ul>` : "<p>No required scenario failed.</p>"}
</section>`;
};

// A file that the page cannot show throws, so that the status line tells
// it.
export function render(report) {
  if (report?.schema !== 1) {
    throw new Error(`The results file has schema ${JSON.stringify(report?.schema) ?? "none"}. The page can show only schema 1.`);
  }
  const { version, commit, target } = report.ncp ?? {};
  if (!isCommit(commit) || typeof report.date !== "string" || !/^\d{4}-\d{2}-\d{2}T/.test(report.date)
    || typeof version !== "string" || typeof target !== "string" || typeof report.suite?.version !== "string"
    || !Array.isArray(report.runs) || !report.runs.length
    || report.runs.some((run) => typeof run.revision !== "string" || !Array.isArray(run.scenarios) || !run.scenarios.length)) {
    throw new Error("The results file has no complete test run details.");
  }
  const known = link(`${repo}/blob/${commit}/harness/conformance/expected-failures.yaml`, "list of known failures");
  // Newest revision first, because it is the revision that ncp targets.
  const runs = [...report.runs].reverse();
  const rows = rowsOf(runs);
  return `<section class="stack"><h2>Tested run</h2>
<dl>
<dt>Tested commit</dt><dd>${link(`${repo}/commit/${commit}`, commit)}</dd>
<dt>Test date</dt><dd>${escape(report.date.slice(0, 10))} (UTC)</dd>
<dt>Version</dt><dd>${escape(version)}, from <code>Cargo.toml</code></dd>
<dt>Build</dt><dd>ncp-cli, the test build of ncp for ${escape(target)}. It has a fake Keychain, a fake browser and a fake TTY.</dd>
<dt>Suite</dt><dd>${escape(report.suite.version)}</dd>
</dl>
</section>
<p>This table shows two requirement runs. It does not show the six other suite runs, which can fail. A full score means all required scenarios in that run passed. Non-required scenarios can fail.</p>
${matrix(runs, rows)}
${failures(runs, rows, known)}
<section class="stack"><h2>Terms</h2>
<dl>
<dt>${result("passed")}</dt><dd>Passed.</dd>
<dt>${result("failed")}</dt><dd>Failed.</dd>
<dt>${result("warning")}</dt><dd>The check has a warning. A warning fails the scenario.</dd>
<dt class="none">${marks.none}</dt><dd>The revision does not have the scenario.</dd>
<dt>required</dt><dd>The revision requires the scenario. Only these scenarios count for conformance.</dd>
<dt>extension</dt><dd>The scenario tests an optional extension of MCP.</dd>
<dt>added-after-release</dt><dd>The suite added the scenario after MCP published the revision.</dd>
<dt>pending</dt><dd>The reference client of the suite cannot pass the scenario yet.</dd>
</dl>
</section>`;
}
