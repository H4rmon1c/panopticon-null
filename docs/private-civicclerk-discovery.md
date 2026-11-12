# Private CivicClerk reconnaissance

`scripts/private-civicclerk-discovery.py` is an explicitly invoked, bounded fallback for municipal websites whose captured first-party pages link CivicClerk portals. It reads a prepared owner-only source archive containing a seed JSON packet (default `private-probe-results-v3.json`, selectable with `--seed-name`), content-addressed source bytes, and capture observations. The packet contains a `results` array of private first-party discovery records with `jurisdiction_ref`, `directory`, `source_anchor`, `portal_url`, `portal_sha256`, and `state=html_screened`; the adapter independently revalidates the captured source chain and does not trust the packet as a finding. It verifies the committed municipal directory and any archived canonical redirect chain, the exact first-party source-page anchor, the portal capture, and pinned copies of the portal's public API client scripts. A changed client version fails closed until inspected and updated. Multiple source-linked tenants for one municipality, or one tenant linked by multiple selected municipalities, are withheld as ambiguous.

Place source archives, output archives, backups, and CI artifacts outside every served/static path; owner-only filesystem permissions alone do not protect a directory served by the same user. A run creates a fresh owner-only private archive with preserved source bytes and receipts. It requests one past-events page for up to three explicitly selected source-bound municipal jurisdictions, with a 2 MiB response cap, a 32 MiB total download budget, same-host HTTPS, no authentication, no redirect following, and no retries after HTTP errors. An optional `--fetch-files` captures at most one explicitly published minutes, agenda, or agenda packet PDF per tenant, using the portal's pinned public file route, with an 8 MiB per-file cap. It does not run OCR or screen PDF text. Default behaviour honors robots policies; the explicitly supplied `--ignore-robots` records any override and does not bypass HTTP access controls or create a review attestation.

Metadata and PDFs are private research leads. The linked site does not establish that every event or file concerns the jurisdiction, that a decision occurred, or how anyone voted. List results are a single bounded page; `has_more` is preserved and completeness is not asserted. Missing file metadata is reported as incomplete, not as an empty file list; a no-supported-file result describes only this bounded list and this adapter’s supported routes. Publication, APIs for public alerts, and the ordinary research exporter do not ingest this archive. The CLI has a 90-second Linux signal deadline; delivery can be delayed by blocking kernel/C operations. A timeout can leave an owner-only partial archive without a completed `report/results.json`. Do not run on a private archive subject to concurrent hostile same-owner mutation.

```sh
python3 scripts/private-civicclerk-discovery.py \\
  --source-archive /absolute/path/to/captured/civicclerk-seeds \\
  --output-parent /absolute/path/to/private/parent \\
  --output-name civicclerk-pass-01 --before 2026-11-11 --limit 3 --fetch-files \
  --jurisdiction us-co-place-0824950 --jurisdiction us-co-place-0826600 \
  --jurisdiction us-co-place-0832155
python3 scripts/test-private-civicclerk-discovery.py
```
