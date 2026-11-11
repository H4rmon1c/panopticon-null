# Third-party discovery leads

`python3 scripts/colorado-eff-private-leads.py --data-dir /absolute/private/output --source-db /absolute/private/research.sqlite3`

This bounded, private reconnaissance uses the EFF Atlas of Surveillance Colorado search as a fallback index. It preserves each returned HTML page and network receipt and records the row and link ordinals. Its source links are fetched only for hosts already reached by the local government directory, under the existing public-HTTPS, robots, byte, and rate bounds. An explicit `--include-gov` option also attempts EFF-linked `.gov` hosts as unverified leads; the suffix does not identify the responsible agency. It does not equate a name, EFF entry, fetched link, or reported deployment with a verified jurisdiction, contract, official, or vote. The public exporter does not consume these results. Each potential claim requires separate source and identity verification.

The November 11, 2026 private capture returned 381 EFF entries across four pages, with 148 distinct city strings and 724 source links. Seven of 14 eligible links on previously reached government hosts returned HTTP 200. The EFF entries include technologies outside the contract focus and can overlap, be old, or lack primary evidence. These counts are discovery leads, not coverage or decision counts. The capture retains its own raw bytes and receipts and is separate from the production research database.

A separate opt-in `.gov` run attempted 37 distinct links and preserved 11 HTTP-200 responses. Those links require independent attribution and verification.
