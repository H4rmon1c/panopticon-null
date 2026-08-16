# Privacy and publication

Public availability does not automatically justify republication.

## Non-negotiable boundaries

- Raw sensitive logs and original records remain local.
- Public output contains only information necessary to establish institutional conduct.
- Plate numbers, home addresses, private identities, movement histories, personal contact details, coordinates, Social Security numbers, family information, and similarly sensitive identifiers must not appear.
- Every free-text field is potentially sensitive.
- The project monitors institutions, procurements, contracts, policies, and public decisions—not private citizens.
- No dossiers on activists, officers, employees, residents, or other people.
- No facial recognition and no person-level movement analysis.
- No harassment, doxxing, unauthorized access, or physical interference.

The static site publishes selected citations, not complete extracted records. Meaningful diffs include only cited changed lines. Local evidence and SQLite directories are mode `0700` on Unix; record and database files are mode `0600` where set directly.

Automated checks reject recognized plate labels, email addresses, common street-address forms, Social Security numbers, personal contact/location field labels, coordinates, and movement logs. Pattern matching cannot detect every sensitive value. Before distributing generated files or approving an X draft, a human must inspect every citation and diff. If relevance is uncertain, do not publish.

## Publication allowlists and the human review gate

Publication is governed by structured allowlists and an append-only human review queue, not by automation.

- A `publication_allowlist` states which field categories may appear publicly. An allowlist is a permission, not auto-approval.
- Every public citation and every image excerpt requires a human review decision bound to exact content digests: evidence ID, source digest, locator/geometry, quote, quote digest, rule digest, processing artifact digest, and proposed public fields. Changing any bound value invalidates approval.
- Image excerpts (for example, a rendered highlight of a quoted PDF region) need a separate, explicit review gate.
- Free-text reviewer notes are never published automatically.
- The site, Atom feed, and X pipeline fail closed on pending, rejected, stale, or mismatched decisions.

The review queue is append-only: Pending, Approved, Rejected, NeedsContext, and Superseded states accumulate; a later decision supersedes an earlier one without deleting history.

## HTTP and processing provenance

HTTP provenance never leaks credentials or cookies. `pnull-http` persists only allowlisted headers and never cookies, authorization headers, or bearer tokens. Processing-run environment capture uses an allowlist, not an environment dump, and subprocess logs contain no secrets.

## Private-life protections

No public-person dossiers, no target ranking, and no harassment tooling. The project never infers legal intent automatically and never asserts facts beyond the preserved public record. No legal conclusions are made.

The purpose is to constrain the panopticon, not recreate it with different operators.
