# Colorado geography worklist

Run `python3 web/panopticon-terminal/scripts/build-colorado-worklist.py --output /tmp/colorado-worklist.csv` from the repository. It produces one row for each of the 4,111 archived geography entries. The default report uses the production release-only schedule, which refreshes twelve exact records and selects **no new discovery origins**. The CSV is an offline configuration inventory; it has no private run ledger or source-review state.

`stage` is the furthest repository milestone: `published_case`, `source_verified_manifest`, `pending_manifest`, `calendar_configured`, `document_index_configured`, or `not_started`. This milestone is independent of whether a source is selected for collection. A directory link does not advance it. The separate columns distinguish:

- `configured_calendar_ids` and `document_index_ids`: known source profiles tied to an explicit geography ID.
- `selected_collection_matrix`, `scheduled_calendar_ids`, and `scheduled_document_index_ids`: discovery origins selected in the matrix being reported. Selection does not enable a timer or prove a review exists.
- `discovery_schedule_state`: `selected_in_matrix` or `not_selected`.
- `source_review_state`: `not_checked`, because the offline builder does not inspect private reviews.
- `search_execution_state`: `not_measured`, because the builder does not inspect capture, extraction, or screening outcomes.

The [calendar mapping](../configs/states/co-calendar-geography.v1.json) connects seven configured calendars. The [document-index mapping](../configs/states/co-document-index-geography.v1.json) connects Silver Plume, Moffat, Simla, Del Norte, and Saguache to exact HTML indexes and bounded PDF policies. A configured town index may now be selected in a schedule without breaking worklist generation. The builder validates the selected matrix and rejects discovery origins with no geography mapping.

Inspect the optional five-town schedule explicitly:

```console
python3 web/panopticon-terminal/scripts/build-colorado-worklist.py \
  --collection-matrix deploy/selfhost/collection-small-town-sources.tsv \
  --output /tmp/colorado-small-town-worklist.csv
```

This reports five selected document indexes, with review state still `not_checked` and search execution still `not_measured`. To report the broader seven-calendar profile, pass `deploy/selfhost/collection-sources.tsv`. A live server's actual selected matrix should be supplied explicitly; this command does not infer systemd activation from a configuration file.

The [CML directory](colorado-municipal-directory.md) supplies 241 website leads among 273 Census municipalities; 28 listed entries have no link and four Census entries were absent. HTTP leads are unverified. A municipal homepage is not an official records endpoint, a source-policy attestation, a relevant decision, or an identified official portrait.

There are 4,099 `not_started` rows: one state, 62 counties, 263 municipalities, 178 school districts, and 3,595 provisional DOLA districts. The other twelve rows are four municipalities with published cases (Colorado Springs, Commerce City, Denver, Castle Rock), Arapahoe County with pending manifests, Douglas County and Littleton with configured calendars, and the five configured town indexes. Separately, 4,107 roster entries have no published decision. Neither number establishes an absence of surveillance contracts. State agencies, tribal governments, universities, public authorities, and other bodies still require additional sourced registries; provisional district identities need verification before attributing a case.
