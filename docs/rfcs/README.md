# RFCs

An `ncp` RFC records the reasoning and the design of a change before the
build.

## Form

- Path: `docs/rfcs/NNNN-slug.md`. Use the next free number. Do not use a
  number again.
- Front matter: `state` (`proposed`, `accepted`, `committed` or
  `abandoned`) and `supersedes` (a list of numbers).
- Sections, in the style of an Oxide RFD: Background, Problems,
  Determinations, Alternatives (optional), Tests (optional) and Compatibility (optional).
  Alternatives gives each design that the RFC does not choose, with the reason from
  the order of choice (`docs/command-development.md`).
- When uv or mise has prior art for the design, run them and quote
  their output with the version. Help text changes between versions, so
  memory is not a source.
- Name each choice that does not follow uv and mise, because the
  maintainer must decide it.
- Before a large edit, make sure that a Git revision or a draft file
  holds the current text. After the edit, ask a reviewer to compare the
  two and list each lost fact.

## States

- Write a new RFC with `state: proposed`. Only the maintainer accepts or
  abandons an RFC. Set `state: accepted` or `state: abandoned` only after
  the maintainer says so.
- Only an `accepted` RFC gives direction. A `proposed` RFC is a draft.
- Do not start to build the change of an RFC before the RFC has
  `state: accepted`.
- An RFC on a separate branch stays editable until its completed
  implementation lands on `main`. A local commit does not freeze it.
  Keep its state `proposed` or `accepted`, as the maintainer decides.
- When the completed implementation lands on `main`, set
  `state: committed`. Update the affected documents in the same change.
  The RFC is then frozen.
- Do not change a frozen or `abandoned` RFC. A new design after that
  needs a new RFC with `supersedes`.
