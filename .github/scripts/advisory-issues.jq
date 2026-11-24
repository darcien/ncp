# Prints the ID, the title and the body of one issue for each advisory ID in
# the report of `cargo deny check advisories`.

def code: "`\(.)`";
def note($prefix): [.notes[] | select(startswith($prefix)) | ltrimstr($prefix)] | first;
def crate: .Krate | "\(.name) \(.version)" + (if .kind then " (\(.kind))" else "" end);
def tree($indent):
  (.parents // []) as $parents
  | range($parents | length) as $i
  | ($i == ($parents | length) - 1) as $last
  | $parents[$i]
  | $indent + (if $last then "└── " else "├── " end) + crate + (if .repeat then " (*)" else "" end),
    tree($indent + (if $last then "    " else "│   " end));

map(select(.type == "diagnostic" and .fields.severity == "error" and .fields.advisory.id))
| group_by(.fields.advisory.id)[]
| .[0].fields as $f
| $f.advisory as $a
| [.[].fields.graphs[]] as $graphs
# The license CC-BY-4.0 requires the original URL. Else the URL of the ID
# comes first, because `url` usually points to a bug tracker.
| (if $a.license == "CC-BY-4.0" then $a.url // ($f | note("Advisory: "))
   else ($f | note("Advisory: ")) // $a.url end) as $url
| {
    id: $a.id,
    title: "\($a.id): \($a.package): \($a.title)",
    body: [
      "- Crate: \($a.package | code)",
      "- Version: \([$graphs[].Krate.version] | unique | map(code) | join(", "))",
      (select($f.code != "vulnerability") | "- Warning: \($f.code | code)"),
      "- Title: \($a.title)",
      "- Date: \($a.date)",
      "- ID: \($a.id | code)",
      ($url | values | "- URL: \(.)"),
      ($f | note("Solution: ") // empty | sub(" \\(try .*\\)$"; "")
        | if startswith("Upgrade to ") then
            "Upgrade to \(ltrimstr("Upgrade to ") | split(" OR ") | map(code) | join(" OR ")) (patched)"
          else . end
        | "- Solution: \(.)"),
      "",
      "Dependency tree:",
      "",
      "```text",
      ($graphs[] | crate, tree("")),
      "```"
    ] | join("\n")
  }
