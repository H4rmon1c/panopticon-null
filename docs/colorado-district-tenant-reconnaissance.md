# Special-district tenant reconnaissance

`python3 scripts/colorado_district_tenants.py --data-dir /absolute/private/path --limit 200 --workers 8`

This private one-off pass joins the pinned DOLA directory to exact provisional
geography IDs and names. It visits distinct non-root HTTPS directory URLs and
requires one visible heading that matches the district's full directory name.
Only directly linked, same-host agenda and minutes document candidates are
recorded, with the index digest and anchor ordinal. Fetches and receipts use the
existing bounded public-HTTPS collector and archive. Duplicate tenant paths,
shared exact pages, unsafe paths, cross-host document links, failed heading
checks, and redirects are excluded.

The resulting `tenant-leads.json` is private research, not the public atlas or
an assertion that a document belongs to a particular legal entity. Even a
matching heading can appear on an index that aggregates several districts.
Before public attribution or automatic publication, a tenant-aware adapter must
prove the exact identity and ownership of each document, revalidate the saved
index and receipt, and handle corrections. The existing exporter does not read
this output. A site check or empty lead list does not establish that no record
exists. The pass refuses to overwrite an existing results file; use a fresh
private data directory for each run.

To fetch and screen only the source-bound direct candidates, run
`python3 scripts/colorado_screen_tenant_leads.py --data-dir /absolute/private/path --pnull-bin /absolute/path/to/pnull`.
This reopens each archived index, rechecks the exact directory binding, receipt,
heading, and link ordinal, and uses the existing bounded sandbox extractor. It
writes a separate private `tenant-screening.json`, without publishing findings.
