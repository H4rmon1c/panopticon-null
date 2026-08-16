-- Committed minimal v0.0.1 database fixture.
--
-- This is the canonical v0.0.1 SQLite schema exactly as Panopticon Null 0.0.1
-- created it (no user_version pragma, treated as version 0). The migration
-- test loads this file into a fresh database and then runs the v0.0.2
-- migration, proving that every original record survives upgrade unchanged.
--
-- The fixture is intentionally minimal but covers each canonical v0.0.1
-- record family: evidence, findings, alerts, approvals, posts, source-fetch
-- history, and post segments. Original evidence IDs and content digests must
-- remain byte-for-byte identical after migration.

CREATE TABLE evidence (
  id TEXT PRIMARY KEY,
  sha256 TEXT NOT NULL,
  source_url TEXT NOT NULL,
  record_json TEXT NOT NULL,
  extracted_text TEXT NOT NULL
);
CREATE TABLE findings (
  id TEXT PRIMARY KEY,
  evidence_id TEXT NOT NULL REFERENCES evidence(id),
  finding_json TEXT NOT NULL
);
CREATE TABLE alerts (
  id TEXT PRIMARY KEY,
  evidence_id TEXT NOT NULL REFERENCES evidence(id),
  alert_json TEXT NOT NULL
);
CREATE TABLE approvals (
  alert_id TEXT PRIMARY KEY REFERENCES alerts(id),
  draft_digest TEXT NOT NULL,
  approved_at TEXT NOT NULL
);
CREATE TABLE posts (
  alert_id TEXT PRIMARY KEY REFERENCES alerts(id),
  remote_ids_json TEXT NOT NULL,
  posted_at TEXT NOT NULL
);
CREATE TABLE source_fetches (
  source_id TEXT PRIMARY KEY,
  fetched_at_unix INTEGER NOT NULL
);
CREATE TABLE post_segments (
  alert_id TEXT NOT NULL REFERENCES posts(alert_id),
  segment_index INTEGER NOT NULL,
  remote_id TEXT NOT NULL,
  PRIMARY KEY(alert_id, segment_index)
);

-- A single v0.0.1 evidence record with its original bytes digest.
INSERT INTO evidence(id, sha256, source_url, record_json, extracted_text)
VALUES (
  'evidence:0136f043bcf653166033290ffa1522d406360e7b6345b4852af92e1739c584c3',
  'badda12921d29bf2fc2d86b274efc9544fa339db82de830ba460eaa9c6bbd2e4',
  'https://coloradosprings.legistar.com/EventDetail.aspx?ID=2654&GUID=ABC',
  '{"kind":"official_api","source_url":"https://coloradosprings.legistar.com/EventDetail.aspx?ID=2654&GUID=ABC","fetched_at":"2026-08-01T00:00:00Z"}',
  'Ordinance No. 25-93, Police Department Technology Surcharge.'
);

-- Its deterministic surveillance finding.
INSERT INTO findings(id, evidence_id, finding_json)
VALUES (
  'finding:0000000000000000000000000000000000000000000000000000000000000000',
  'evidence:0136f043bcf653166033290ffa1522d406360e7b6345b4852af92e1739c584c3',
  '{"kind":"finding","rule_id":"rule:surveillance-tech","evidence_id":"evidence:0136f043bcf653166033290ffa1522d406360e7b6345b4852af92e1739c584c3","locator":{"kind":"line","start":100,"end":110,"label":"line 100-110"},"matched_rules":["vendor.axon","vendor.flock-safety"],"state":"surveillance_technology","action":"scheduled_for_public_hearing"}'
);

-- Its alert.
INSERT INTO alerts(id, evidence_id, alert_json)
VALUES (
  'alert:94c9526803b6e122e8b4f31a05f21fe84535b948812d253d2817bc9f6a71b2c6',
  'evidence:0136f043bcf653166033290ffa1522d406360e7b6345b4852af92e1739c584c3',
  '{"id":"alert:94c9526803b6e122e8b4f31a05f21fe84535b948812d253d2817bc9f6a71b2c6","evidence_id":"evidence:0136f043bcf653166033290ffa1522d406360e7b6345b4852af92e1739c584c3","state":"scheduled_for_public_hearing"}'
);

-- Its approval and post history.
INSERT INTO approvals(alert_id, draft_digest, approved_at)
VALUES (
  'alert:94c9526803b6e122e8b4f31a05f21fe84535b948812d253d2817bc9f6a71b2c6',
  'demo-draft-digest',
  '2026-08-02T00:00:00Z'
);
INSERT INTO posts(alert_id, remote_ids_json, posted_at)
VALUES (
  'alert:94c9526803b6e122e8b4f31a05f21fe84535b948812d253d2817bc9f6a71b2c6',
  '["segment-1","segment-2"]',
  '2026-08-02T00:00:01Z'
);
INSERT INTO post_segments(alert_id, segment_index, remote_id)
VALUES
  ('alert:94c9526803b6e122e8b4f31a05f21fe84535b948812d253d2817bc9f6a71b2c6', 0, 'segment-1'),
  ('alert:94c9526803b6e122e8b4f31a05f21fe84535b948812d253d2817bc9f6a71b2c6', 1, 'segment-2');

-- Source-fetch history.
INSERT INTO source_fetches(source_id, fetched_at_unix)
VALUES ('colorado-springs-legistar-events', 1754000000);
