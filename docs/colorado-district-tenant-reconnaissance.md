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

`python3 scripts/colorado_tenant_followup.py --data-dir /absolute/private/path --pnull-bin /absolute/path/to/pnull`
also follows up to three clearly labeled meeting or document navigation links
from each verified directory tenant page. The linked page must be on the same
HTTPS host, return a direct HTML response, and repeat the exact full district
heading. Each direct document keeps the captured secondary index digest and
anchor ordinal; the output also preserves the parent page and navigation
anchor. The existing sandbox screens those documents with a shared download
budget and no automatic public attribution. This command writes a separate
`tenant-secondary-screening.json` and refuses to overwrite it on a later run.

## Shared-root sites

`python3 scripts/colorado_shared_districts.py --data-dir /absolute/private/path --offset 0 --limit 80`
groups distinct provisional district IDs from the pinned directory that advertise
the same root host. It fetches each root once and records names visible on the
page and direct same-host agenda/minutes anchors. A visible name does not make
every document on a shared site that district's record. The private output is
`shared-host-leads-OFFSET.json`; use a fresh offset to cover subsequent hosts.

`python3 scripts/colorado_screen_shared_leads.py --data-dir /absolute/private/path --pnull-bin /absolute/path/to/pnull --offset 0`
rechecks the directory group, original page receipt and each anchor before
sandbox screening. Results go to `shared-host-screening-OFFSET.json`. The
extractor uses an internal shared-site sentinel, not a real jurisdiction;
neither the lead nor its screen establishes district ownership, a vote or a
decision, and neither command publishes anything. Further source-specific
attribution is required before public use.
