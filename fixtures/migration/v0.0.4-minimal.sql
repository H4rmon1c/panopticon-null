-- Committed minimal v0.0.4 database fixture (schema version 3).
-- Covers every v0.0.3 procurement-domain table plus the v0.0.4 public-ledger
-- tables (procurement_alerts, cora_requests, official_relationships,
-- supplied_records) with representative rows. The migration test loads this
-- and upgrades to v0.0.4c (version 4) proving every canonical record survives
-- byte-for-byte and all v0.0.4 rows are preserved, and that no snapshot-row
-- history is fabricated for snapshots whose rows were never preserved.
PRAGMA foreign_keys=OFF;
BEGIN TRANSACTION;
CREATE TABLE evidence (
  id TEXT PRIMARY KEY,
  sha256 TEXT NOT NULL,
  source_url TEXT NOT NULL,
  record_json TEXT NOT NULL,
  extracted_text TEXT NOT NULL
);
INSERT INTO evidence VALUES('evidence:0136f043bcf653166033290ffa1522d406360e7b6345b4852af92e1739c584c3','badda12921d29bf2fc2d86b274efc9544fa339db82de830ba460eaa9c6bbd2e4','https://coloradosprings.legistar.com/EventDetail.aspx?ID=2654&GUID=ABC','{"kind":"official_api"}','Ordinance No. 25-93, Police Department Technology Surcharge.');
CREATE TABLE findings (
  id TEXT PRIMARY KEY,
  evidence_id TEXT NOT NULL REFERENCES evidence(id),
  finding_json TEXT NOT NULL
);
INSERT INTO findings VALUES('finding:0000000000000000000000000000000000000000000000000000000000000000','evidence:0136f043bcf653166033290ffa1522d406360e7b6345b4852af92e1739c584c3','{"kind":"finding","state":"surveillance_technology"}');
CREATE TABLE alerts (
  id TEXT PRIMARY KEY,
  evidence_id TEXT NOT NULL REFERENCES evidence(id),
  alert_json TEXT NOT NULL
);
INSERT INTO alerts VALUES('alert:94c9526803b6e122e8b4f31a05f21fe84535b948812d253d2817bc9f6a71b2c6','evidence:0136f043bcf653166033290ffa1522d406360e7b6345b4852af92e1739c584c3','{"id":"alert:94c9526803b6e122e8b4f31a05f21fe84535b948812d253d2817bc9f6a71b2c6","state":"scheduled_for_public_hearing"}');
CREATE TABLE approvals (
  alert_id TEXT PRIMARY KEY REFERENCES alerts(id),
  draft_digest TEXT NOT NULL,
  approved_at TEXT NOT NULL
);
INSERT INTO approvals VALUES('alert:94c9526803b6e122e8b4f31a05f21fe84535b948812d253d2817bc9f6a71b2c6','demo-draft-digest','2026-08-02T00:00:00Z');
CREATE TABLE posts (
  alert_id TEXT PRIMARY KEY REFERENCES alerts(id),
  remote_ids_json TEXT NOT NULL,
  posted_at TEXT NOT NULL
);
INSERT INTO posts VALUES('alert:94c9526803b6e122e8b4f31a05f21fe84535b948812d253d2817bc9f6a71b2c6','["segment-1","segment-2"]','2026-08-02T00:00:01Z');
CREATE TABLE source_fetches (
  source_id TEXT PRIMARY KEY,
  fetched_at_unix INTEGER NOT NULL
);
INSERT INTO source_fetches VALUES('colorado-springs-legistar-events',1754000000);
CREATE TABLE post_segments (
  alert_id TEXT NOT NULL REFERENCES posts(alert_id),
  segment_index INTEGER NOT NULL,
  remote_id TEXT NOT NULL,
  PRIMARY KEY(alert_id, segment_index)
);
INSERT INTO post_segments VALUES('alert:94c9526803b6e122e8b4f31a05f21fe84535b948812d253d2817bc9f6a71b2c6',0,'segment-1');
INSERT INTO post_segments VALUES('alert:94c9526803b6e122e8b4f31a05f21fe84535b948812d253d2817bc9f6a71b2c6',1,'segment-2');
CREATE TABLE matters (
  id TEXT PRIMARY KEY,
  source_id TEXT NOT NULL,
  official_matter_id TEXT NOT NULL,
  matter_json TEXT NOT NULL
);
INSERT INTO matters VALUES('matter:co-25-93','co','25-93','{"id":"matter:co-25-93","official_matter_id":"25-93","title":"Ordinance No. 25-93","status":"adopted","document_role":"ordinance"}');
CREATE TABLE matter_attachments (
  id TEXT PRIMARY KEY,
  matter_id TEXT NOT NULL REFERENCES matters(id),
  attachment_json TEXT NOT NULL
);
INSERT INTO matter_attachments VALUES('attachment:1','matter:co-25-93','{"id":"attachment:1","matter_id":"matter:co-25-93","official_id":"att1","name":"draft.pdf","url":"https://example.test/draft.pdf"}');
CREATE TABLE subjects (
  id TEXT PRIMARY KEY,
  matter_id TEXT NOT NULL,
  subject_json TEXT NOT NULL
);
INSERT INTO subjects VALUES('subject:1','matter:co-25-93','{"id":"subject:1","matter_id":"matter:co-25-93","kind":"surveillance_technology","name":"Surveillance Technology","known":true}');
CREATE TABLE actions (
  id TEXT PRIMARY KEY,
  matter_id TEXT NOT NULL,
  subject_id TEXT NOT NULL,
  action_json TEXT NOT NULL
);
INSERT INTO actions VALUES('action:1','matter:co-25-93','subject:1','{"id":"action:1","matter_id":"matter:co-25-93","subject_id":"subject:1","kind":"approved","summary":"Council approved","known":true}');
CREATE TABLE text_maps (
  id TEXT PRIMARY KEY,
  evidence_id TEXT NOT NULL REFERENCES evidence(id),
  text_map_json TEXT NOT NULL
);
CREATE TABLE page_citations (
  id TEXT PRIMARY KEY,
  evidence_id TEXT NOT NULL REFERENCES evidence(id),
  page_citation_json TEXT NOT NULL
);
CREATE TABLE review_decisions (
  id TEXT PRIMARY KEY,
  citation_id TEXT NOT NULL,
  decision_json TEXT NOT NULL
);
CREATE TABLE processing_runs (
  id TEXT PRIMARY KEY,
  run_json TEXT NOT NULL
);
INSERT INTO processing_runs VALUES('run:1','{"id":"run:1","schema_version":3,"pnull_version":"0.0.4","outcome":"complete"}');
CREATE TABLE source_reviews (
  id TEXT PRIMARY KEY,
  source_id TEXT NOT NULL,
  review_json TEXT NOT NULL
);
INSERT INTO source_reviews VALUES('sr:1','co','{"id":"sr:1","source_id":"co","reviewer":"operator","note":"approved"}');
CREATE TABLE fetch_observations (
  id TEXT PRIMARY KEY,
  source_id TEXT,
  observation_json TEXT NOT NULL
);
INSERT INTO fetch_observations VALUES('fo:1','co','{"id":"fo:1","source_id":"co","status_code":200}');
CREATE TABLE x_attempts (
  id TEXT PRIMARY KEY,
  alert_id TEXT NOT NULL REFERENCES alerts(id),
  attempt_json TEXT NOT NULL
);
INSERT INTO x_attempts VALUES('xa:1','alert:94c9526803b6e122e8b4f31a05f21fe84535b948812d253d2817bc9f6a71b2c6','{"id":"xa:1","status":"posted"}');
CREATE TABLE x_reconciliations (
  id TEXT PRIMARY KEY,
  attempt_id TEXT NOT NULL REFERENCES x_attempts(id),
  reconciliation_json TEXT NOT NULL
);
INSERT INTO x_reconciliations VALUES('xr:1','xa:1','{"id":"xr:1","decision":"confirmed"}');
CREATE TABLE publication_allowlists (
  id TEXT PRIMARY KEY,
  allowlist_json TEXT NOT NULL
);
INSERT INTO publication_allowlists VALUES('allow:1','{"id":"allow:1","field_categories":["quote","locator"]}');
-- v0.0.3 procurement-domain tables.
CREATE TABLE procurement_matters (
  id TEXT PRIMARY KEY,
  official_title TEXT NOT NULL,
  matter_json TEXT NOT NULL
);
INSERT INTO procurement_matters VALUES('proc:matter:co:r26-023ab','R26-023AB — Next-Generation Transit Fare Collection System (RFI)','{"id":"proc:matter:co:r26-023ab","jurisdiction":"Colorado Springs","title":"R26-023AB — Next-Generation Transit Fare Collection System (RFI)","review_state":"draft","publication_state":"unpublished"}');
CREATE TABLE procurement_events (
  id TEXT PRIMARY KEY,
  matter_id TEXT NOT NULL,
  event_json TEXT NOT NULL
);
INSERT INTO procurement_events VALUES('proc-event:1','proc:matter:co:r26-023ab','{"id":"proc-event:1","matter_id":"proc:matter:co:r26-023ab","kind":"solicitation_published","date":"2025-12-01","summary":"RFI published","identifier_ids":[],"evidence_ids":[],"source_id":"colorado-springs-solicitation-mirror"}');
CREATE TABLE procurement_identifiers (
  id TEXT PRIMARY KEY,
  matter_id TEXT NOT NULL,
  identifier_json TEXT NOT NULL
);
INSERT INTO procurement_identifiers VALUES('proc-id:1','proc:matter:co:r26-023ab','{"id":"proc-id:1","matter_id":"proc:matter:co:r26-023ab","kind":"solicitation_number","raw":"R26-023AB","source_id":"colorado-springs-solicitation-mirror","normalized":"R26023AB","normalization_rule":"uppercase-alphanumeric-compact","known":false}');
CREATE TABLE procurement_organizations (
  id TEXT PRIMARY KEY,
  matter_id TEXT NOT NULL,
  organization_json TEXT NOT NULL
);
INSERT INTO procurement_organizations VALUES('proc-org:1','proc:matter:co:r26-023ab','{"id":"proc-org:1","matter_id":"proc:matter:co:r26-023ab","role":"government_department","raw_name":"Colorado Springs Mountain Metropolitan Transit","source_id":"colorado-springs-solicitation-mirror","normalized_alias":null,"alias_reviewed":false}');
CREATE TABLE coverage_ledger (
  id TEXT PRIMARY KEY,
  source_id TEXT NOT NULL,
  entry_json TEXT NOT NULL
);
INSERT INTO coverage_ledger VALUES('coverage:1','colorado-springs-contract-awards','{"id":"coverage:1","source_id":"colorado-springs-contract-awards","source_url":"https://coloradosprings.gov/procurement-services/page/contract-award-information","authority":"official_informational_mirror","state":"informational_only","retrieved_at":"2026-08-17T00:00:00Z","persisted_digest":"aa","http_status":null,"etag":null,"last_modified":null,"final_url":null,"parser_version":"awards-1.0","schema_version":3,"claimed_date_range":null,"record_count":8,"pagination_complete":null,"access_errors":[],"human_review_state":"unreviewed","note":"snapshot captured"}');
CREATE TABLE source_snapshots (
  id TEXT PRIMARY KEY,
  source_id TEXT NOT NULL,
  snapshot_json TEXT NOT NULL
);
INSERT INTO source_snapshots VALUES('snapshot:1','colorado-springs-contract-awards','{"id":"snapshot:1","source_id":"colorado-springs-contract-awards","source_url":"https://coloradosprings.gov/procurement-services/page/contract-award-information","retrieved_at":"2026-08-17T00:00:00Z","persisted_digest":"aa","content_type":"text/html","etag":null,"last_modified":null,"final_url":"https://coloradosprings.gov/procurement-services/page/contract-award-information","redirect_history":[],"parser_version":"awards-1.0","schema_version":3,"record_count":8,"pagination_complete":null,"coverage_state":"informational_only","supersedes":null}');
CREATE TABLE snapshot_revisions (
  id TEXT PRIMARY KEY,
  snapshot_id TEXT NOT NULL,
  revision_json TEXT NOT NULL
);
CREATE TABLE snapshot_diffs (
  id TEXT PRIMARY KEY,
  old_snapshot_id TEXT NOT NULL,
  new_snapshot_id TEXT NOT NULL,
  diff_json TEXT NOT NULL
);
CREATE TABLE reconciliation_items (
  id TEXT PRIMARY KEY,
  matter_id TEXT NOT NULL,
  item_json TEXT NOT NULL
);
INSERT INTO reconciliation_items VALUES('reconcile:1','proc:matter:co:r26-023ab','{"id":"reconcile:1","matter_id":"proc:matter:co:r26-023ab","kind":"missing_document","summary":"executed contract for R26-023AB (not observed in checked sources)","record_refs":[],"state":"open","created_at":"2026-08-17T00:00:00Z"}');
CREATE TABLE reconciliation_decisions (
  id TEXT PRIMARY KEY,
  item_id TEXT NOT NULL,
  decision_json TEXT NOT NULL
);
CREATE TABLE case_files (
  id TEXT PRIMARY KEY,
  matter_id TEXT NOT NULL,
  case_file_json TEXT NOT NULL
);
INSERT INTO case_files VALUES('case-file:1','proc:matter:co:r26-023ab','{"id":"case-file:1","matter_id":"proc:matter:co:r26-023ab","state":"draft","json_digest":"jj","markdown_digest":"mm","sha256_manifest":[["case-file.json","jj"],["case-file.md","mm"]],"built_at":"2026-08-17T00:00:00Z"}');
CREATE TABLE cora_drafts (
  id TEXT PRIMARY KEY,
  matter_id TEXT NOT NULL,
  draft_json TEXT NOT NULL
);
INSERT INTO cora_drafts VALUES('cora:1','proc:matter:co:r26-023ab','{"id":"cora:1","matter_id":"proc:matter:co:r26-023ab","institution":"City of Colorado Springs","identifiers":["R26-023AB"],"missing_record_types":["executed contract"],"date_range":null,"vendor_or_project":null,"sources_checked":["colorado-springs-contract-awards"],"markdown":"# Draft","created_at":"2026-08-17T00:00:00Z"}');
-- v0.0.4 public-ledger tables.
CREATE TABLE procurement_alerts (
  id TEXT PRIMARY KEY,
  source_id TEXT NOT NULL,
  alert_json TEXT NOT NULL
);
INSERT INTO procurement_alerts VALUES('proc-alert:1','colorado-springs-contract-awards','{"id":"proc-alert:1","source_id":"colorado-springs-contract-awards","surface":"contract-award-table","old_snapshot_id":"snapshot:old","old_snapshot_digest":"aaaa","new_snapshot_id":"snapshot:new","new_snapshot_digest":"bbbb","retrieved_at":"2026-08-17T00:00:00Z","coverage_state":"informational_only","summary":"A row not present in snapshot snapshot:old now appears in snapshot snapshot:new.","changes":[{"change_kind":"record_added","row_identity":"r1","field_diffs":[],"old_snapshot_id":"snapshot:old","old_snapshot_digest":"aaaa","new_snapshot_id":"snapshot:new","new_snapshot_digest":"bbbb","summary":"appeared"}],"matter_ids":["proc:matter:co:r26-023ab"],"identifier_ids":[],"taxonomy_matches":[]}');
CREATE TABLE cora_requests (
  id TEXT PRIMARY KEY,
  matter_id TEXT NOT NULL,
  request_json TEXT NOT NULL
);
INSERT INTO cora_requests VALUES('cora-req:1','proc:matter:co:r26-023ab','{"id":"cora-req:1","matter_id":"proc:matter:co:r26-023ab","state":"submitted","submitted_at":"2026-08-17T00:00:00Z"}');
CREATE TABLE official_relationships (
  id TEXT PRIMARY KEY,
  source_record_id TEXT NOT NULL,
  relationship_json TEXT NOT NULL
);
INSERT INTO official_relationships VALUES('official-rel:1','proc:matter:co:r26-023ab:record:award:0','{"id":"official-rel:1","kind":"official_relationship","source_record_id":"proc:matter:co:r26-023ab:record:award:0","source_snapshot_id":"snapshot:1","source_snapshot_digest":"aa","target_identifier":"R26-023AB","target_matter_id":"proc:matter:co:r26-023ab","reference_field":"notes","quote":"R26-023AB","locator":"contract-award row notes column","citations":["c1"],"reviewed":true}');
CREATE TABLE supplied_records (
  id TEXT PRIMARY KEY,
  observed_digest TEXT NOT NULL,
  record_json TEXT NOT NULL
);
INSERT INTO supplied_records VALUES('supplied:1','dddd','{"id":"supplied:1","observed_digest":"dddd","kind":"award","source_id":"colorado-springs-contract-awards"}');
COMMIT;

PRAGMA user_version = 3;
