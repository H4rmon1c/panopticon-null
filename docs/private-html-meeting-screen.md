# Private HTML meeting-page screening

`scripts/private-html-meeting-screen.py` rechecks a small, already captured private HTML reconnaissance batch. It does not fetch pages, schedule work, populate the research database, or publish claims. The Linux CLI uses a 60-second signal deadline and withholds its fresh output on timeout; Python signal delivery can be delayed by a blocking kernel call or C code. The owner-only archive must already contain `private-results.json` (directory discovery records), `private-html-screen-results.json` (captured page candidates), and the collector's content-addressed `content` and `observations` directories. It creates a fresh owner-only child output directory and refuses to overwrite it.

For each candidate, the adapter binds the municipality to the committed directory, validates any captured official-domain redirect chain, requires same-authority HTTPS source/index URLs and preserved successful HTML receipts, verifies both content hashes, and checks the exact index anchor resolves to the captured page. It recomputes preliminary rule matches from HTML text nodes after excluding scripts, templates, deleted content and common inline hidden markup; output supplies each matching preliminary text node's zero-based locator and the source/index digests and receipt digests. Full directory provenance and capture metadata remain private. The output is not a `research.sqlite3` store and the ordinary exporter does not ingest it. Page size, parser events, nesting, text nodes, receipt count and bytes, matches, and output bytes are bounded; exceeding a per-page limit withholds that page.

This is a conservative offline first pass, not a full browser rendering or a completeness check. External stylesheets, inline CSS forms not recognized by this conservative parser, malformed HTML, inaccessible documents, embedded files, images, OCR, split text nodes, responsible body, and the meaning or disposition of any match can remain unresolved. A link from a municipality's site does not establish which body made a decision. Review source and governing-body attribution before any public use. Run only on owner-controlled archives without concurrent mutation; it is not a race-resistant hostile-same-owner ingestion boundary.

For a prepared private archive:

```sh
python3 scripts/private-html-meeting-screen.py --archive /absolute/path/to/private/archive --output-name html-screen-01
python3 scripts/test-private-html-meeting-screen.py
```
