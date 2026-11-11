# Fixed Colorado document archive profiles

These profiles prepare official records for private, content-addressed archiving. They are **not** active collection-matrix rows, source reviews, or public case releases. The site never calls these endpoints from the browser. Each endpoint requires its own current source-level review before `pnull source archive` can fetch it. See [the archive operator guide](colorado-direct-archive.md).

| Profile | Fixed sources | Key boundary |
| --- | ---: | --- |
| `configs/states/co-commerce-archive.toml` | 7 | Ordinances 2755 and 2756 have separate direct roll calls. Attached agreement copies do not establish City execution. |
| `configs/states/co-castle-rock-archive.toml` | 6 | `castle-rock-res2025-071-event-json` and `castle-rock-res2025-071-votes-json` are the **direct** RES 2025-071 event and vote records. The agreement attachment has no Town execution signature in the preserved copy. |
| `configs/states/co-denver-archive.toml` | 12 | `denver-25-0413-event-json` and `denver-25-0413-votes-json` document a **failed** Flock amendment. The Axon 25-1967/1968 `consent-votes-json` is a block vote, not a direct roll call on either resolution. |
| `configs/states/co-arapahoe-archive.toml` | 14 | Both Flock matters were on consent agendas. Their archived `consent-votes-json` records belong to the block motions, not standalone Flock item votes. |

Each profile also includes official HTML/PDF minutes, detail, or agreement records where available. The exact URLs correspond to preserved bytes and SHA-256 digests in `fixtures/atlas/sources.co.v1.json`; the offline `python3 deploy/selfhost/test-archive-profiles.py` check verifies all 39 configured endpoints and their source-config digests against those local bytes. A current endpoint response may differ from the preserved snapshot; the archive command records such a change in private state and does not itself change any public claim. Five configured attachment URLs on `legistar.granicus.com`, `legistar2.granicus.com`, and `denver.legistar1.com` currently have a host-wide `Disallow: /` robots response. They remain historical fixtures and must not be put into an automated collection schedule. The four source-verified release manifests cite 12 other endpoints after removing those five supplemental citations; their field-level proofs still validate.

For an individual reviewed endpoint, run:

```sh
pnull --config configs/states/co-castle-rock-archive.toml \
  --data-dir /var/lib/panopticon source archive castle-rock-res2025-071-event-json
```

The source-review commands and limits are documented in `docs/colorado-direct-archive.md`. Add archive rows to `deploy/selfhost/collection-sources.tsv` only after the corresponding reviews are recorded. One source-policy review does not mean a person must review each fetched record. Ten meeting/PDF path policies are configured separately for new calendar links; these are operator-triggered and do not yet schedule or turn candidates into cases. See [document-first discovery](document-first-discovery.md).
