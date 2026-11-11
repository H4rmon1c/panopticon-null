# Private county-root reconnaissance

This opt-in tool looks for county-hosted document leads when the existing
research snapshot records a successful county website check but no completed
document, portal, or agenda-page screening. The regular discovery path does not
enable the root probe. The normal directory lead is often a clerk's subpage;
the probe additionally visits the same host's HTTPS root within the existing
page and download limits. It retains the original directory lead and source
receipts. The probe does not adopt legislative portal links, including Denver's.

Run against a private research store and snapshot, choosing a fresh private
output directory:

```sh
python3 scripts/colorado_county_root_recon.py \
  --private-dir /absolute/fresh/private/output \
  --source-db /absolute/private/research.sqlite3 \
  --snapshot /absolute/private/continuous-research-snapshot.json
```

The output contains unverified county-hosted leads only in a JSON archive;
it does not write collector database rows, so the normal exporter cannot ingest
those leads. It does not establish which public body a document
concerns. A county website can host records of municipalities, separate boards,
or other bodies. Before considering ordinary discovery or publication, establish
the responsible agency and decision from source evidence and preserve those
relationships in the release model. Do not attribute a decision, contract, vote,
or absence of one to county commissioners simply because of the host.

In the isolated 2026-11-11 exploratory pass, 17 previously reached but
unscreened county sites were attempted and reached. Nine linked documents were
identified across Eagle and Pueblo, including duplicates, leading to six unique
final documents screened without a keyword match. Four initial candidate URLs
redirected and were separately followed to those final documents. This is a
bounded discovery result, not a complete search of those counties. No decision
or case was established.
