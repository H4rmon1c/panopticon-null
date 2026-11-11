//! Deterministic, source-backed static data for the public atlas.
//!
//! The legacy case uses a case-level release review. Version 2 cases verify
//! preserved bytes, exact claims, reviewed source captures, and private fetch
//! receipts before production release. Preview output is explicitly marked.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path};

use pnull_core::{ReviewState, Store, sha256_hex, stable_id};
use serde::Deserialize;
use serde_json::{Value, json};
use thiserror::Error;
use url::Url;

use crate::{publication_allowlist_allows, validate_public_text};

mod v2;

pub const ATLAS_RELEASE_CATEGORY: &str = "atlas_release";
const MAX_FIXTURE_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AtlasExportMode {
    Preview,
    Production,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AtlasReviewRequirement {
    pub id: String,
    pub bound_digest: String,
    pub kind: String,
    pub summary: String,
    pub source_url: String,
    pub locator: String,
    pub source_sha256: String,
}

#[derive(Debug, Error)]
pub enum AtlasError {
    #[error("atlas I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("atlas JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("atlas store failed: {0}")]
    Store(#[from] pnull_core::CoreError),
    #[error("atlas manifest invalid: {0}")]
    Manifest(String),
    #[error("atlas source verification failed: {0}")]
    Source(String),
    #[error("atlas review gate failed: {0}")]
    Review(String),
    #[error("atlas privacy gate failed: {0}")]
    Privacy(String),
}

#[derive(Deserialize)]
struct Manifest {
    schema_version: u32,
    case_id: String,
    synthetic: bool,
    review_state: String,
    jurisdiction: NamedRecord,
    agency: AgencyRecord,
    decision: DecisionRecord,
    votes: Vec<VoteRecord>,
    contract_link: MissingLink,
    technology_context: TechnologyContext,
    unresolved_question: String,
}

#[derive(Deserialize)]
struct NamedRecord {
    id: String,
    name: String,
    proof: Proof,
}

#[derive(Deserialize)]
struct AgencyRecord {
    id: String,
    name: String,
    relationship: String,
    proof: Proof,
}

#[derive(Deserialize)]
struct DecisionRecord {
    official_matter: String,
    ordinance: String,
    body: String,
    date: String,
    action: String,
    outcome: String,
    tally: Tally,
    summary: String,
    proofs: Vec<Proof>,
}

#[derive(Deserialize)]
struct Tally {
    aye: u32,
    no: u32,
    abstain: u32,
    absent: u32,
}

#[derive(Deserialize)]
struct VoteRecord {
    person_name: String,
    recorded_vote: String,
    official_vote_id: u64,
    official_person_id: u64,
    office_role: MissingLink,
    office_term: MissingLink,
    proof: Proof,
}

#[derive(Deserialize)]
struct MissingLink {
    state: String,
    #[serde(default)]
    detail: String,
}

#[derive(Deserialize)]
struct TechnologyContext {
    relationship: String,
    systems: Vec<String>,
    proof: Proof,
}

#[derive(Clone, Deserialize)]
struct Proof {
    url: String,
    sha256: String,
    fixture: String,
    #[serde(default)]
    locator: String,
    #[serde(default)]
    locators: Vec<String>,
    #[serde(default)]
    quote: String,
    #[serde(default)]
    name_locator: String,
    #[serde(default)]
    name_quote: String,
    #[serde(default)]
    vote_locator: String,
    #[serde(default)]
    vote_quote: String,
    #[serde(default)]
    event_item_locator: String,
    #[serde(default)]
    event_item_quote: String,
    #[serde(default)]
    person_id_locator: String,
    #[serde(default)]
    person_id_quote: String,
}

impl Proof {
    fn primary_locator(&self) -> &str {
        if !self.locator.is_empty() {
            &self.locator
        } else if !self.vote_locator.is_empty() {
            &self.vote_locator
        } else if !self.quote.is_empty() {
            self.locators
                .iter()
                .find(|locator| locator.ends_with("ActionText"))
                .or_else(|| self.locators.first())
                .map_or("", String::as_str)
        } else {
            self.locators.first().map_or("", String::as_str)
        }
    }
}

struct Claim {
    key: String,
    summary: String,
    proof: Proof,
    quote: String,
    locator: String,
}

struct Validated {
    manifest: Manifest,
    manifest_digest: String,
    claims: Vec<Claim>,
    sources: BTreeMap<String, Value>,
}

/// Inspect the exact case-level release binding a human must approve. Merely
/// calling this function never records a review or creates an allowlist.
pub fn review_requirements(
    manifest_path: &Path,
) -> Result<Vec<AtlasReviewRequirement>, AtlasError> {
    if manifest_version(manifest_path)? == 2 {
        return Ok(vec![v2::requirement(&v2::inspect(manifest_path)?)]);
    }
    let checked = validate_manifest(manifest_path)?;
    Ok(requirements(&checked))
}

/// Export a private preview or an approved public snapshot. The caller chooses
/// the destination; production validates all gates before touching the file.
pub fn export_atlas(
    store: &Store,
    manifest_path: &Path,
    output_path: &Path,
    mode: AtlasExportMode,
    site_url: Option<&str>,
) -> Result<(), AtlasError> {
    let snapshot = build_snapshot(store, manifest_path, mode, site_url)?;
    write_snapshot(&snapshot, output_path)
}

/// Export several independently verified cases as one deterministic atlas.
/// All cases pass their own release gate before the destination is replaced.
#[allow(clippy::too_many_lines)]
pub fn export_atlas_bundle(
    store: &Store,
    manifests: &[std::path::PathBuf],
    output_path: &Path,
    mode: AtlasExportMode,
    site_url: Option<&str>,
) -> Result<(), AtlasError> {
    if manifests.is_empty() {
        return Err(AtlasError::Manifest(
            "bundle has no case manifests".to_owned(),
        ));
    }
    if manifests.len() == 1 {
        return export_atlas(store, &manifests[0], output_path, mode, site_url);
    }
    if manifests.len() > 256 {
        return Err(AtlasError::Manifest("bundle exceeds 256 cases".to_owned()));
    }
    let mut cases = Vec::new();
    let mut seen_cases = BTreeSet::new();
    for path in manifests {
        let snapshot = build_snapshot(store, path, mode, site_url)?;
        let id = snapshot["entities"]
            .as_array()
            .and_then(|entities| entities.iter().find(|entity| entity["type"] == "DECISION"))
            .and_then(|entity| entity["slug"].as_str())
            .ok_or_else(|| AtlasError::Manifest("case lacks a decision slug".to_owned()))?
            .to_owned();
        if !seen_cases.insert(id.clone()) {
            return Err(AtlasError::Manifest(format!(
                "duplicate case in bundle: {id}"
            )));
        }
        cases.push((id, snapshot));
    }
    cases.sort_by(|left, right| left.0.cmp(&right.0));
    let demo = cases[0].1["demo"].as_bool().unwrap_or(false);
    if cases.iter().any(|(_, value)| value["demo"] != demo) {
        return Err(AtlasError::Manifest(
            "synthetic and official cases cannot share a bundle".to_owned(),
        ));
    }
    let mut sources = BTreeMap::<String, Value>::new();
    let mut evidence = BTreeMap::<String, Value>::new();
    let mut entities = BTreeMap::<String, Value>::new();
    let mut release = Vec::new();
    let mut bindings = Vec::new();
    for (case_id, snapshot) in cases {
        let case_review = snapshot["release_review"].clone();
        bindings.push(
            case_review["bound_digest"]
                .as_str()
                .unwrap_or("")
                .to_owned(),
        );
        release.push(json!({"case_id":case_id,"release":case_review}));
        for source in snapshot["sources"]
            .as_array()
            .expect("verified snapshot sources")
        {
            insert_consistent(&mut sources, source, "source")?;
        }
        for claim in snapshot["evidence"]
            .as_array()
            .expect("verified snapshot evidence")
        {
            insert_consistent(&mut evidence, claim, "evidence")?;
        }
        for entity in snapshot["entities"]
            .as_array()
            .expect("verified snapshot entities")
        {
            let id = entity["id"].as_str().unwrap_or("").to_owned();
            if let Some(existing) = entities.get_mut(&id) {
                merge_shared_entity(existing, entity)?;
            } else {
                entities.insert(id, entity.clone());
            }
        }
    }
    let parts = bindings.iter().map(String::as_str).collect::<Vec<_>>();
    let digest = review_digest("atlas-bundle-binding", &parts);
    let is_preview = mode == AtlasExportMode::Preview;
    let all_human_approved = release
        .iter()
        .all(|item| item["release"]["state"] == "APPROVED");
    let reviewed_at = if !is_preview && all_human_approved {
        release
            .iter()
            .filter_map(|item| item["release"]["reviewed_at"].as_str())
            .max()
            .map(str::to_owned)
    } else {
        None
    };
    let release_review = json!({
        "state":if is_preview {"PENDING_RELEASE"} else if all_human_approved {"APPROVED"} else {"SOURCE_VERIFIED"},
        "bound_digest":digest,"reviewed_at":reviewed_at,"corrections":[],
        "verification_method":if !is_preview && !all_human_approved {
            Some("preserved_source_bytes_and_exact_locators")
        } else {None},
    });
    let relationship_count = entities
        .values()
        .map(|entity| entity["relationships"].as_array().map_or(0, Vec::len))
        .sum::<usize>();
    let output = json!({
        "schema_version":"pnull-public-snapshot-v1","demo":demo,"preview":is_preview,
        "status":{"state":if is_preview {"pending_release"} else {"released"},
            "dataset_version":digest,"last_ingest":null,"last_publish":null,
            "manifest_digest":digest,
            "counts":{"entities":entities.len(),"records":evidence.len(),
                "sources":sources.len(),"relationships":relationship_count}},
        "release_review":release_review,"case_releases":release,
        "activity":[],"entities":entities.into_values().collect::<Vec<_>>(),
        "evidence":evidence.into_values().collect::<Vec<_>>(),
        "sources":sources.into_values().collect::<Vec<_>>(),
    });
    write_snapshot(&output, output_path)
}

/// Publish active cases while retaining a minimal, review-bound notice at
/// each withdrawn case's former share URL. Reviewer notes stay private.
pub fn export_atlas_bundle_with_withdrawals(
    store: &Store,
    manifests: &[std::path::PathBuf],
    withdrawn: &[std::path::PathBuf],
    output_path: &Path,
    mode: AtlasExportMode,
    site_url: Option<&str>,
) -> Result<(), AtlasError> {
    if withdrawn.is_empty() {
        return export_atlas_bundle(store, manifests, output_path, mode, site_url);
    }
    if mode != AtlasExportMode::Production || withdrawn.len() > 256 {
        return Err(AtlasError::Manifest(
            "withdrawn manifests require a bounded production release".to_owned(),
        ));
    }
    let staging = tempfile::tempdir()?;
    let staged_path = staging.path().join("active.json");
    export_atlas_bundle(store, manifests, &staged_path, mode, site_url)?;
    let mut snapshot: Value = serde_json::from_slice(&fs::read(&staged_path)?)?;
    let active = snapshot["case_releases"]
        .as_array()
        .ok_or_else(|| AtlasError::Manifest("active bundle lacks case releases".to_owned()))?
        .iter()
        .filter_map(|item| item["case_id"].as_str())
        .collect::<BTreeSet<_>>();
    let mut notices = BTreeMap::<String, Value>::new();
    for path in withdrawn {
        let raw: Value = serde_json::from_slice(&fs::read(path)?)?;
        let case_id = raw["case_id"]
            .as_str()
            .ok_or_else(|| AtlasError::Manifest("withdrawn case lacks case_id".to_owned()))?;
        if !case_id.starts_with("co-") || active.contains(case_id) || notices.contains_key(case_id)
        {
            return Err(AtlasError::Manifest(
                "withdrawn case is duplicated or still active".to_owned(),
            ));
        }
        let requirements = review_requirements(path)?;
        if requirements.len() != 1 {
            return Err(AtlasError::Review(
                "withdrawn case lacks one release binding".to_owned(),
            ));
        }
        let requirement = &requirements[0];
        let review = store.current_review(&requirement.id)?.ok_or_else(|| {
            AtlasError::Review("withdrawn case lacks a current rejection".to_owned())
        })?;
        if review.state != ReviewState::Rejected
            || review.bound_digest != requirement.bound_digest
            || !human_reviewer(&review.reviewer)
        {
            return Err(AtlasError::Review(
                "withdrawn case needs a current human rejection of this exact binding".to_owned(),
            ));
        }
        notices.insert(
            case_id.to_owned(),
            json!({
                "case_id":case_id,"state":"withdrawn","decided_at":review.decided_at,
                "bound_digest":requirement.bound_digest,
                "review_id":review.id,
            }),
        );
    }
    let old = snapshot["release_review"]["bound_digest"]
        .as_str()
        .ok_or_else(|| AtlasError::Manifest("bundle lacks release binding".to_owned()))?
        .to_owned();
    let notices = notices.into_values().collect::<Vec<_>>();
    let parts = std::iter::once(old.as_str())
        .chain(
            notices
                .iter()
                .filter_map(|notice| notice["review_id"].as_str()),
        )
        .collect::<Vec<_>>();
    let digest = review_digest("atlas-bundle-withdrawals", &parts);
    snapshot["withdrawals"] = json!(notices);
    snapshot["release_review"]["bound_digest"] = json!(digest);
    snapshot["status"]["dataset_version"] = json!(digest);
    snapshot["status"]["manifest_digest"] = json!(digest);
    write_snapshot(&snapshot, output_path)
}

/// Preserve a reviewed withdrawal notice for a case from the previous public
/// snapshot, including proposal-backed cases that have no v2 manifest.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub fn export_atlas_bundle_with_prior_withdrawals(
    store: &Store,
    manifests: &[std::path::PathBuf],
    withdrawn_manifests: &[std::path::PathBuf],
    withdrawn_case_ids: &[String],
    previous_snapshot: Option<&Path>,
    output_path: &Path,
    mode: AtlasExportMode,
    site_url: Option<&str>,
) -> Result<(), AtlasError> {
    if withdrawn_case_ids.is_empty() {
        return export_atlas_bundle_with_withdrawals(
            store,
            manifests,
            withdrawn_manifests,
            output_path,
            mode,
            site_url,
        );
    }
    if mode != AtlasExportMode::Production || withdrawn_case_ids.len() > 256 {
        return Err(AtlasError::Manifest(
            "prior withdrawals require production".to_owned(),
        ));
    }
    let prior_path = previous_snapshot.ok_or_else(|| {
        AtlasError::Manifest("prior withdrawals need a previous public snapshot".to_owned())
    })?;
    let previous: Value = serde_json::from_slice(&fs::read(prior_path)?)?;
    if previous["demo"] != false
        || previous["preview"] != false
        || previous["status"]["state"] != "released"
    {
        return Err(AtlasError::Manifest(
            "previous snapshot is not a public release".to_owned(),
        ));
    }
    let prior_releases = previous["case_releases"]
        .as_array()
        .ok_or_else(|| AtlasError::Manifest("previous release lacks case bindings".to_owned()))?;
    let staging = tempfile::tempdir()?;
    let staged_path = staging.path().join("active.json");
    export_atlas_bundle_with_withdrawals(
        store,
        manifests,
        withdrawn_manifests,
        &staged_path,
        mode,
        site_url,
    )?;
    let mut snapshot: Value = serde_json::from_slice(&fs::read(&staged_path)?)?;
    let mut notices = snapshot["withdrawals"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut seen = notices
        .iter()
        .filter_map(|item| item["case_id"].as_str().map(str::to_owned))
        .collect::<BTreeSet<_>>();
    for case_id in withdrawn_case_ids {
        if !case_id.starts_with("co-")
            || !seen.insert(case_id.clone())
            || snapshot["case_releases"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item["case_id"] == case_id.as_str()))
        {
            return Err(AtlasError::Manifest(
                "prior withdrawal is duplicate or active".to_owned(),
            ));
        }
        let prior_release = prior_releases
            .iter()
            .find(|item| item["case_id"] == case_id.as_str());
        let prior_notice = previous["withdrawals"].as_array().and_then(|items| {
            items
                .iter()
                .find(|item| item["case_id"] == case_id.as_str() && item["state"] == "withdrawn")
        });
        let binding = if let Some(prior) = prior_release {
            if !previous["entities"].as_array().is_some_and(|items| {
                items
                    .iter()
                    .any(|item| item["type"] == "DECISION" && item["slug"] == case_id.as_str())
            }) {
                return Err(AtlasError::Manifest("prior case lacks decision".to_owned()));
            }
            prior["release"]["bound_digest"].as_str()
        } else {
            prior_notice.and_then(|item| item["bound_digest"].as_str())
        }
        .ok_or_else(|| AtlasError::Manifest("case is absent from previous release".to_owned()))?;
        let review_id = stable_id("atlas-release", &[case_id]);
        let review = store
            .current_review(&review_id)?
            .ok_or_else(|| AtlasError::Review("prior case lacks withdrawal review".to_owned()))?;
        if review.state != ReviewState::Rejected
            || review.bound_digest != binding
            || !human_reviewer(&review.reviewer)
        {
            return Err(AtlasError::Review(
                "prior case lacks exact human rejection".to_owned(),
            ));
        }
        notices.push(json!({"case_id":case_id,"state":"withdrawn",
            "decided_at":review.decided_at,"bound_digest":binding,"review_id":review.id}));
    }
    notices.sort_by(|a, b| a["case_id"].as_str().cmp(&b["case_id"].as_str()));
    let old = snapshot["release_review"]["bound_digest"]
        .as_str()
        .ok_or_else(|| AtlasError::Manifest("release lacks binding".to_owned()))?
        .to_owned();
    let parts = std::iter::once(old.as_str())
        .chain(notices.iter().filter_map(|item| item["review_id"].as_str()))
        .collect::<Vec<_>>();
    let digest = review_digest("atlas-bundle-prior-withdrawals", &parts);
    snapshot["withdrawals"] = json!(notices);
    snapshot["release_review"]["bound_digest"] = json!(digest);
    snapshot["status"]["dataset_version"] = json!(digest);
    snapshot["status"]["manifest_digest"] = json!(digest);
    write_snapshot(&snapshot, output_path)
}

/// Append a proposal snapshot already rebuilt from reviewed candidate
/// archives by the CLI. This function checks bundle identity and consistency;
/// the caller must perform the source revalidation before calling it.
#[allow(clippy::too_many_lines)]
pub fn append_verified_proposal_snapshot(
    base: &mut Value,
    extra: &Value,
) -> Result<(), AtlasError> {
    if base["demo"] != false
        || base["preview"] != false
        || extra["demo"] != false
        || extra["preview"] != false
        || base["status"]["state"] != "released"
        || extra["status"]["state"] != "released"
    {
        return Err(AtlasError::Manifest(
            "proposal cannot join a nonpublic bundle".to_owned(),
        ));
    }
    let releases = extra["case_releases"]
        .as_array()
        .ok_or_else(|| AtlasError::Manifest("proposal lacks case release".to_owned()))?;
    if releases.len() != 1 || releases[0]["release"]["state"] != "SOURCE_VERIFIED" {
        return Err(AtlasError::Manifest(
            "proposal lacks one source-verified release".to_owned(),
        ));
    }
    let case_id = releases[0]["case_id"]
        .as_str()
        .ok_or_else(|| AtlasError::Manifest("proposal release lacks case ID".to_owned()))?;
    if !extra["entities"].as_array().is_some_and(|entities| {
        entities
            .iter()
            .filter(|entity| entity["type"] == "DECISION" && entity["slug"] == case_id)
            .count()
            == 1
    }) {
        return Err(AtlasError::Manifest(
            "proposal lacks its exact decision".to_owned(),
        ));
    }
    let withdrawn_collision = base["withdrawals"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item["case_id"] == case_id));
    let existing = base["case_releases"]
        .as_array_mut()
        .ok_or_else(|| AtlasError::Manifest("bundle lacks case releases".to_owned()))?;
    if existing.iter().any(|item| item["case_id"] == case_id) || withdrawn_collision {
        return Err(AtlasError::Manifest(
            "proposal case ID already exists in bundle".to_owned(),
        ));
    }
    existing.push(releases[0].clone());
    existing.sort_by(|a, b| a["case_id"].as_str().cmp(&b["case_id"].as_str()));
    for key in ["sources", "evidence", "entities"] {
        let target = base[key]
            .as_array_mut()
            .ok_or_else(|| AtlasError::Manifest(format!("bundle lacks {key}")))?;
        let incoming = extra[key]
            .as_array()
            .ok_or_else(|| AtlasError::Manifest(format!("proposal lacks {key}")))?;
        let mut ids = target
            .iter()
            .filter_map(|item| item["id"].as_str().map(str::to_owned))
            .collect::<BTreeSet<_>>();
        for item in incoming {
            let id = item["id"]
                .as_str()
                .ok_or_else(|| AtlasError::Manifest(format!("proposal {key} lacks id")))?;
            if !ids.insert(id.to_owned()) {
                return Err(AtlasError::Manifest(format!(
                    "proposal {key} ID collides: {id}"
                )));
            }
            target.push(item.clone());
        }
        target.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    }
    let bindings = base["case_releases"]
        .as_array()
        .expect("checked releases")
        .iter()
        .map(|item| item["release"]["bound_digest"].as_str().unwrap_or(""))
        .collect::<Vec<_>>();
    if bindings.iter().any(|digest| !digest.starts_with("sha256:")) {
        return Err(AtlasError::Manifest(
            "bundle has an invalid case binding".to_owned(),
        ));
    }
    let withdrawals = base["withdrawals"]
        .as_array()
        .map_or_else(Vec::new, |items| {
            items
                .iter()
                .filter_map(|item| item["review_id"].as_str())
                .collect::<Vec<_>>()
        });
    let digest = review_digest(
        "atlas-bundle-with-proposals",
        &bindings.into_iter().chain(withdrawals).collect::<Vec<_>>(),
    );
    base["release_review"] = json!({"state":"SOURCE_VERIFIED","bound_digest":digest,
        "reviewed_at":null,"corrections":[],
        "verification_method":"preserved_source_bytes_and_exact_locators"});
    let relationships = base["entities"]
        .as_array()
        .expect("checked entities")
        .iter()
        .map(|entity| entity["relationships"].as_array().map_or(0, Vec::len))
        .sum::<usize>();
    base["status"]["counts"] = json!({"entities":base["entities"].as_array().expect("entities").len(),
        "records":base["evidence"].as_array().expect("evidence").len(),
        "sources":base["sources"].as_array().expect("sources").len(),"relationships":relationships});
    base["status"]["dataset_version"] = json!(digest);
    base["status"]["manifest_digest"] = json!(digest);
    validate_value(base)?;
    Ok(())
}

/// Persist a bundle assembled only from the gated atlas exporter and freshly
/// rebuilt proposal snapshots.
pub fn write_verified_atlas_snapshot(
    snapshot: &Value,
    output_path: &Path,
) -> Result<(), AtlasError> {
    write_snapshot(snapshot, output_path)
}

fn insert_consistent(
    collection: &mut BTreeMap<String, Value>,
    item: &Value,
    kind: &str,
) -> Result<(), AtlasError> {
    let id = item["id"]
        .as_str()
        .ok_or_else(|| AtlasError::Manifest(format!("{kind} lacks id")))?;
    if let Some(previous) = collection.get(id) {
        if previous != item {
            return Err(AtlasError::Manifest(format!(
                "conflicting {kind} id in bundle: {id}"
            )));
        }
    } else {
        collection.insert(id.to_owned(), item.clone());
    }
    Ok(())
}

fn merge_shared_entity(existing: &mut Value, next: &Value) -> Result<(), AtlasError> {
    let kind = existing["type"].as_str().unwrap_or("");
    let voter = kind == "RECORDED_VOTER";
    if !matches!(kind, "JURISDICTION" | "AGENCY" | "RECORDED_VOTER")
        || next["type"] != kind
        || next["name"] != existing["name"]
        || next["jurisdiction_ref"] != existing["jurisdiction_ref"]
        || next["geo"] != existing["geo"]
        || next["status"] != existing["status"]
        || (voter
            && (next["identity_scope"] != existing["identity_scope"]
                || next["official_person_id"] != existing["official_person_id"]
                || next["jurisdiction_ids"] != existing["jurisdiction_ids"]
                || next["description"] != existing["description"]
                || next["subtitle"] != existing["subtitle"]))
    {
        return Err(AtlasError::Manifest(format!(
            "conflicting entity id in bundle: {}",
            existing["id"]
        )));
    }
    let mut ids = existing["source_ids"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    ids.extend(next["source_ids"].as_array().cloned().unwrap_or_default());
    ids.sort_by(|left, right| left.as_str().cmp(&right.as_str()));
    ids.dedup();
    existing["source_count"] = json!(ids.len());
    existing["source_ids"] = json!(ids);
    let mut attributes = existing["attributes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for attribute in next["attributes"].as_array().cloned().unwrap_or_default() {
        if !attributes.contains(&attribute) {
            attributes.push(attribute);
        }
    }
    existing["attributes"] = json!(attributes);
    if voter {
        let mut timeline = existing["timeline"].as_array().cloned().unwrap_or_default();
        for event in next["timeline"].as_array().cloned().unwrap_or_default() {
            if let Some(previous) = timeline.iter().find(|item| item["id"] == event["id"]) {
                if previous != &event {
                    return Err(AtlasError::Manifest(format!(
                        "conflicting recorded voter timeline in bundle: {}",
                        existing["id"]
                    )));
                }
            } else {
                timeline.push(event);
            }
        }
        timeline.sort_by(|left, right| {
            left["date"]
                .as_str()
                .cmp(&right["date"].as_str())
                .then_with(|| left["id"].as_str().cmp(&right["id"].as_str()))
        });
        existing["timeline"] = json!(timeline);
    }
    if next["updated_at"].as_str() > existing["updated_at"].as_str() {
        existing["updated_at"] = next["updated_at"].clone();
    }
    Ok(())
}

fn manifest_version(path: &Path) -> Result<u64, AtlasError> {
    let header: Value = serde_json::from_slice(&fs::read(path)?)?;
    header["schema_version"]
        .as_u64()
        .ok_or_else(|| AtlasError::Manifest("manifest lacks numeric schema_version".to_owned()))
}

fn build_snapshot(
    store: &Store,
    manifest_path: &Path,
    mode: AtlasExportMode,
    site_url: Option<&str>,
) -> Result<Value, AtlasError> {
    if manifest_version(manifest_path)? == 2 {
        let checked = v2::inspect(manifest_path)?;
        if mode == AtlasExportMode::Production {
            v2::production_gate(store, &checked, site_url)?;
        }
        return v2::snapshot(store, &checked, mode, site_url);
    }
    let checked = validate_manifest(manifest_path)?;
    let required = requirements(&checked);
    if mode == AtlasExportMode::Production {
        production_gate(store, &checked, &required, site_url)?;
    }
    snapshot(store, &checked, &required, mode, site_url)
}

fn write_snapshot(snapshot: &Value, output_path: &Path) -> Result<(), AtlasError> {
    validate_value(snapshot)?;
    let mut bytes = serde_json::to_vec_pretty(&snapshot)?;
    bytes.push(b'\n');
    let parent = output_path
        .parent()
        .ok_or_else(|| AtlasError::Manifest("output has no parent".to_owned()))?;
    fs::create_dir_all(parent)?;
    // A fixed temporary name can be pre-created as a symlink and can also be
    // shared by concurrent preview and production exports.
    let mut temporary = tempfile::Builder::new()
        .prefix(".pnull-atlas-")
        .tempfile_in(parent)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(output_path)
        .map_err(|error| AtlasError::Io(error.error))?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn validate_manifest(path: &Path) -> Result<Validated, AtlasError> {
    let path = path.canonicalize()?;
    let raw = fs::read(&path)?;
    let manifest_digest = sha256_hex(&raw);
    let manifest: Manifest = serde_json::from_slice(&raw)?;
    if manifest.schema_version != 1 || manifest.case_id != "co-colorado-springs-25-581-2025-11-25" {
        return Err(AtlasError::Manifest(
            "unsupported manifest version or case id".to_owned(),
        ));
    }
    if !matches!(manifest.review_state.as_str(), "pending" | "approved") {
        return Err(AtlasError::Manifest(
            "review_state must be pending or approved".to_owned(),
        ));
    }
    if manifest.contract_link.state != "not_established"
        || manifest.votes.len() != 9
        || manifest.votes.iter().any(|v| {
            v.office_role.state != "not_established" || v.office_term.state != "not_established"
        })
    {
        return Err(AtlasError::Manifest(
            "unsupported contract, vote, or office-role assertion".to_owned(),
        ));
    }
    if manifest.decision.official_matter != "25-581"
        || manifest.decision.ordinance != "25-93"
        || manifest.decision.date != "2025-11-25"
        || manifest.decision.action != "finally passed"
        || manifest.decision.outcome != "Pass"
        || manifest.decision.tally.aye != 6
        || manifest.decision.tally.no != 3
        || manifest.decision.tally.abstain != 0
        || manifest.decision.tally.absent != 0
    {
        return Err(AtlasError::Manifest(
            "decision identity or tally differs from preserved case".to_owned(),
        ));
    }
    if manifest.jurisdiction.id != "co-colorado-springs"
        || manifest.agency.id != "colorado-springs-police-department"
        || manifest.decision.proofs.len() != 2
        || manifest.technology_context.systems.is_empty()
    {
        return Err(AtlasError::Manifest(
            "required case records are missing".to_owned(),
        ));
    }
    validate_case_claims(&manifest)?;
    validate_case_proof_bindings(&manifest)?;
    let mut claims = vec![
        Claim::new(
            "jurisdiction",
            &manifest.jurisdiction.name,
            &manifest.jurisdiction.proof,
        ),
        Claim::new(
            "agency",
            &manifest.agency.relationship,
            &manifest.agency.proof,
        ),
        Claim::new(
            "decision_action",
            &manifest.decision.summary,
            &manifest.decision.proofs[0],
        ),
        Claim::new(
            "decision_ordinance",
            "Signed Ordinance 25-93 amends city code to assess the Police Department Technology Surcharge",
            &manifest.decision.proofs[1],
        ),
    ];
    for vote in &manifest.votes {
        claims.push(Claim::new(
            &format!("vote_{}", vote.official_vote_id),
            &format!(
                "{} recorded {} on Ordinance 25-93",
                vote.person_name, vote.recorded_vote
            ),
            &vote.proof,
        ));
    }
    claims.push(Claim::new(
        "technology_context",
        &manifest.technology_context.relationship,
        &manifest.technology_context.proof,
    ));
    let mut sources = BTreeMap::new();
    for claim in &claims {
        let source = verify_proof(&path, &claim.proof)?;
        let id = source_id(&claim.proof);
        if let Some(existing) = sources.insert(id.clone(), source.clone())
            && existing != source
        {
            return Err(AtlasError::Source(format!(
                "conflicting proofs for source {id}"
            )));
        }
    }
    verify_case_facts(&path, &manifest)?;
    Ok(Validated {
        manifest,
        manifest_digest,
        claims,
        sources,
    })
}

impl Claim {
    fn new(key: &str, summary: &str, proof: &Proof) -> Self {
        let locator = if !proof.name_locator.is_empty() && !proof.vote_locator.is_empty() {
            format!("{}; {}", proof.name_locator, proof.vote_locator)
        } else {
            proof.primary_locator().to_owned()
        };
        Self {
            key: key.to_owned(),
            summary: summary.to_owned(),
            proof: proof.clone(),
            quote: proof.quote.clone(),
            locator,
        }
    }
}

// Schema v1 covers one reviewed case. These source-backed fields stay fixed
// until a new manifest version has its own verification rules.
fn validate_case_claims(manifest: &Manifest) -> Result<(), AtlasError> {
    let systems = [
        "Body Worn Camera (Axon)",
        "Digital Evidence System & Storage (Axon)",
        "AI Transcription & Reporting (Axon)",
        "Vehicle Intelligence Cameras (Flock)",
    ];
    if manifest.jurisdiction.name != "Colorado Springs, Colorado"
        || manifest.agency.name != "Colorado Springs Police Department"
        || manifest.agency.relationship != "agency named in signed ordinance"
        || manifest.decision.body != "Colorado Springs City Council"
        || manifest.decision.summary
            != "The City Council finally passed Ordinance 25-93, amending city code to assess a Police Department Technology Surcharge."
        || manifest.technology_context.relationship
            != "systems named in a supporting staff cost presentation; not contracts approved by this vote"
        || !manifest
            .technology_context
            .systems
            .iter()
            .map(String::as_str)
            .eq(systems)
    {
        return Err(AtlasError::Manifest(
            "source-backed case claim differs from approved v1 facts".to_owned(),
        ));
    }
    Ok(())
}

fn validate_case_proof_bindings(manifest: &Manifest) -> Result<(), AtlasError> {
    const ORDINANCE: &str = "fixtures/co/ordinance-25-93-signed.pdf";
    const PRESENTATION: &str = "fixtures/co/police-technology-surcharge-presentation.pdf";
    expect_pdf_proof(
        &manifest.jurisdiction.proof,
        ORDINANCE,
        "PDF p. 1, enacting clause: City Council of the City of Colorado Springs",
        &[],
        "City Council of the City of Colorado Springs",
    )?;
    expect_pdf_proof(
        &manifest.agency.proof,
        ORDINANCE,
        "PDF p. 1, title and Section 1(H)(1)",
        &[],
        "police department technology surcharge",
    )?;
    expect_event_proof(&manifest.decision.proofs[0])?;
    expect_pdf_proof(
        &manifest.decision.proofs[1],
        ORDINANCE,
        "",
        &["PDF p. 1, Section 1(H)(1)", "PDF p. 2, finally passed date"],
        "",
    )?;
    expect_pdf_proof(
        &manifest.technology_context.proof,
        PRESENTATION,
        "PDF p. 7, 2026 Major Systems Costs table",
        &[],
        "",
    )?;
    for vote in &manifest.votes {
        expect_vote_proof(&vote.proof)?;
    }
    Ok(())
}

fn expect_fixture(proof: &Proof, expected: &str) -> Result<(), AtlasError> {
    if proof.fixture == expected {
        Ok(())
    } else {
        Err(AtlasError::Source(format!(
            "proof is bound to wrong preserved record: {}",
            proof.fixture
        )))
    }
}

fn expect_event_proof(proof: &Proof) -> Result<(), AtlasError> {
    expect_fixture(proof, "fixtures/co/event-2660-final-vote.json")?;
    let expected = [
        "$.EventItems[38].EventItemId",
        "$.EventItems[38].EventItemMatterFile",
        "$.EventItems[38].EventItemActionText",
        "$.EventItems[38].EventItemPassedFlagName",
    ];
    if !proof.locator.is_empty()
        || proof.quote.is_empty()
        || !proof.locators.iter().map(String::as_str).eq(expected)
        || !proof.name_locator.is_empty()
        || !proof.name_quote.is_empty()
        || !proof.vote_locator.is_empty()
        || !proof.vote_quote.is_empty()
        || !proof.event_item_locator.is_empty()
        || !proof.event_item_quote.is_empty()
        || !proof.person_id_locator.is_empty()
        || !proof.person_id_quote.is_empty()
    {
        return Err(AtlasError::Source(
            "decision proof locator differs from checked event item".to_owned(),
        ));
    }
    Ok(())
}

fn expect_vote_proof(proof: &Proof) -> Result<(), AtlasError> {
    expect_fixture(proof, "fixtures/co/votes-103550-final-vote.json")?;
    let Some(base) = proof.name_locator.strip_suffix(".VotePersonName") else {
        return Err(AtlasError::Source(
            "vote name locator is invalid".to_owned(),
        ));
    };
    if !base.starts_with("$[")
        || !base.ends_with(']')
        || proof.vote_locator != format!("{base}.VoteValueName")
        || proof.event_item_locator != format!("{base}.VoteEventItemId")
        || proof.person_id_locator != format!("{base}.VotePersonId")
        || proof.name_quote.is_empty()
        || proof.vote_quote.is_empty()
        || proof.event_item_quote.is_empty()
        || proof.person_id_quote.is_empty()
        || !proof.locator.is_empty()
        || !proof.locators.is_empty()
        || !proof.quote.is_empty()
    {
        return Err(AtlasError::Source(
            "vote proof locators do not refer to one roll-call row".to_owned(),
        ));
    }
    Ok(())
}

fn expect_pdf_proof(
    proof: &Proof,
    fixture: &str,
    locator: &str,
    locators: &[&str],
    quote: &str,
) -> Result<(), AtlasError> {
    expect_fixture(proof, fixture)?;
    if proof.locator != locator
        || proof.quote != quote
        || !proof
            .locators
            .iter()
            .map(String::as_str)
            .eq(locators.iter().copied())
        || !proof.name_locator.is_empty()
        || !proof.name_quote.is_empty()
        || !proof.vote_locator.is_empty()
        || !proof.vote_quote.is_empty()
        || !proof.event_item_locator.is_empty()
        || !proof.event_item_quote.is_empty()
        || !proof.person_id_locator.is_empty()
        || !proof.person_id_quote.is_empty()
    {
        return Err(AtlasError::Source(format!(
            "PDF proof differs from approved v1 locator or quote: {}",
            proof.fixture
        )));
    }
    Ok(())
}

fn trusted_source(fixture: &str) -> Option<(&'static str, &'static str, &'static str)> {
    match fixture {
        "fixtures/co/event-2660-final-vote.json" => Some((
            "Colorado Springs City Council final vote event",
            "https://webapi.legistar.com/v1/coloradosprings/events/2660?EventItems=1&AgendaNote=1&MinutesNote=1&EventItemAttachments=1",
            "f11a5712daf21f8f7d345d772995efb1cbeb495da7981bd1a1a6aa22b57a6759",
        )),
        "fixtures/co/votes-103550-final-vote.json" => Some((
            "Colorado Springs City Council roll call",
            "https://webapi.legistar.com/v1/coloradosprings/EventItems/103550/Votes",
            "99bbdb2394a1e30ad7761bea917a0700472604bab613b33b7ce4368327182c80",
        )),
        "fixtures/co/ordinance-25-93-signed.pdf" => Some((
            "Signed Ordinance 25-93",
            "https://coloradosprings.legistar.com/View.ashx?M=F&ID=14995655&GUID=00173B3A-1CD9-4839-9AD5-93B3C8E4EE03",
            "f364d09dbbb29a0b8d89c53002eb4bb757ddf0b942f245f4d70738543b4dc1fb",
        )),
        "fixtures/co/police-technology-surcharge-presentation.pdf" => Some((
            "Police Technology Surcharge staff presentation",
            "https://coloradosprings.legistar.com/View.ashx?M=F&ID=14876735&GUID=EA1466B1-D5D6-48CF-BB70-7710853E363D",
            "7612843a72f957413f5eb7d0be71d27a47406e398816f80219c470e18387e6a8",
        )),
        _ => None,
    }
}

fn fixture_path(manifest_path: &Path, relative: &str) -> Result<std::path::PathBuf, AtlasError> {
    let rel = Path::new(relative);
    if rel.is_absolute()
        || !rel.starts_with("fixtures/co/")
        || rel
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(AtlasError::Source(format!(
            "unsafe fixture path: {relative}"
        )));
    }
    for ancestor in manifest_path.ancestors().skip(1) {
        let candidate = ancestor.join(rel);
        if candidate.is_file() {
            let root = ancestor.canonicalize()?;
            let resolved = candidate.canonicalize()?;
            if resolved.starts_with(root.join("fixtures/co")) {
                return Ok(resolved);
            }
        }
    }
    Err(AtlasError::Source(format!(
        "fixture unavailable: {relative}"
    )))
}

fn read_verified(manifest_path: &Path, proof: &Proof) -> Result<Vec<u8>, AtlasError> {
    let path = fixture_path(manifest_path, &proof.fixture)?;
    let mut raw = Vec::new();
    fs::File::open(&path)?
        .take(MAX_FIXTURE_BYTES + 1)
        .read_to_end(&mut raw)?;
    if raw.len() as u64 > MAX_FIXTURE_BYTES {
        return Err(AtlasError::Source(format!(
            "fixture exceeds byte limit: {}",
            proof.fixture
        )));
    }
    if proof.sha256.len() != 64
        || !proof.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        || sha256_hex(&raw) != proof.sha256
    {
        return Err(AtlasError::Source(format!(
            "digest mismatch: {}",
            proof.fixture
        )));
    }
    Ok(raw)
}

fn verify_proof(manifest_path: &Path, proof: &Proof) -> Result<Value, AtlasError> {
    let url = Url::parse(&proof.url)
        .map_err(|e| AtlasError::Source(format!("invalid source URL: {e}")))?;
    if url.scheme() != "https"
        || url.username() != ""
        || url.password().is_some()
        || !matches!(
            url.host_str(),
            Some("webapi.legistar.com" | "coloradosprings.legistar.com")
        )
    {
        return Err(AtlasError::Source(format!(
            "source is not an official HTTPS record: {}",
            proof.url
        )));
    }
    let Some((title, expected_url, expected_sha256)) = trusted_source(&proof.fixture) else {
        return Err(AtlasError::Source(
            "unrecognized preserved record".to_owned(),
        ));
    };
    if proof.url != expected_url || proof.sha256 != expected_sha256 {
        return Err(AtlasError::Source(format!(
            "source URL or digest differs from approved record: {}",
            proof.fixture
        )));
    }
    let raw = read_verified(manifest_path, proof)?;
    if proof.primary_locator().is_empty() {
        return Err(AtlasError::Source("proof has no exact locator".to_owned()));
    }
    if Path::new(&proof.fixture)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
    {
        let value: Value = serde_json::from_slice(&raw)?;
        for locator in std::iter::once(&proof.locator)
            .chain(proof.locators.iter())
            .chain([
                &proof.name_locator,
                &proof.vote_locator,
                &proof.event_item_locator,
                &proof.person_id_locator,
            ])
        {
            if locator.is_empty() {
                continue;
            }
            if locate(&value, locator).is_none() {
                return Err(AtlasError::Source(format!("locator missing: {locator}")));
            }
        }
        for (locator, quote) in [
            (&proof.locator, &proof.quote),
            (&proof.name_locator, &proof.name_quote),
            (&proof.vote_locator, &proof.vote_quote),
            (&proof.event_item_locator, &proof.event_item_quote),
            (&proof.person_id_locator, &proof.person_id_quote),
        ] {
            if !locator.is_empty()
                && !quote.is_empty()
                && scalar_at(&value, locator).as_deref() != Some(quote.as_str())
            {
                return Err(AtlasError::Source(format!("quote differs at {locator}")));
            }
        }
        if !proof.quote.is_empty()
            && proof.locator.is_empty()
            && !proof
                .locators
                .iter()
                .any(|locator| scalar_at(&value, locator).as_deref() == Some(proof.quote.as_str()))
        {
            return Err(AtlasError::Source(
                "quoted decision action differs from preserved JSON".to_owned(),
            ));
        }
    } else if !Path::new(&proof.fixture)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
    {
        return Err(AtlasError::Source("unsupported fixture type".to_owned()));
    }
    Ok(json!({
        "id":source_id(proof), "title":title, "authority":"City of Colorado Springs",
        "source_type":"official_record", "canonical_url":proof.url,
        "retrieved_at":null, "document_date":null, "sha256":proof.sha256,
        "description":"Preserved official record; SHA-256 verified against local raw bytes"
    }))
}

fn locate<'a>(value: &'a Value, locator: &str) -> Option<&'a Value> {
    let mut rest = locator.strip_prefix('$')?;
    let mut current = value;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('.') {
            let end = after.find(['.', '[']).unwrap_or(after.len());
            if end == 0 {
                return None;
            }
            current = current.get(&after[..end])?;
            rest = &after[end..];
        } else if let Some(after) = rest.strip_prefix('[') {
            let end = after.find(']')?;
            current = current.get(after[..end].parse::<usize>().ok()?)?;
            rest = &after[end + 1..];
        } else {
            return None;
        }
    }
    Some(current)
}

fn scalar_at(value: &Value, locator: &str) -> Option<String> {
    match locate(value, locator)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn verify_case_facts(path: &Path, manifest: &Manifest) -> Result<(), AtlasError> {
    let event: Value = serde_json::from_slice(&read_verified(path, &manifest.decision.proofs[0])?)?;
    let item = &event["EventItems"][38];
    if item["EventItemId"] != 103_550
        || item["EventItemMatterFile"] != manifest.decision.official_matter
        || item["EventItemPassedFlagName"] != manifest.decision.outcome
        || !item["EventItemActionText"]
            .as_str()
            .unwrap_or("")
            .contains("6-3-0-0")
        || event["EventBodyName"] != "City Council"
        || item["EventItemActionText"].as_str() != Some(manifest.decision.proofs[0].quote.as_str())
    {
        return Err(AtlasError::Source(
            "decision facts differ from preserved event".to_owned(),
        ));
    }
    let mut counts = BTreeMap::<&str, u32>::new();
    let mut seen = BTreeSet::new();
    for vote in &manifest.votes {
        let raw: Value = serde_json::from_slice(&read_verified(path, &vote.proof)?)?;
        let name = scalar_at(&raw, &vote.proof.name_locator);
        let position = scalar_at(&raw, &vote.proof.vote_locator);
        let person_id = scalar_at(&raw, &vote.proof.person_id_locator);
        let event_id = scalar_at(&raw, &vote.proof.event_item_locator);
        let vote_id_locator = vote
            .proof
            .name_locator
            .replace(".VotePersonName", ".VoteId");
        let vote_id = scalar_at(&raw, &vote_id_locator);
        if name.as_deref() != Some(vote.person_name.as_str())
            || position.as_deref() != Some(vote.recorded_vote.as_str())
            || person_id.as_deref() != Some(vote.official_person_id.to_string().as_str())
            || event_id.as_deref() != Some("103550")
            || vote_id.as_deref() != Some(vote.official_vote_id.to_string().as_str())
            || !seen.insert(vote.official_vote_id)
        {
            return Err(AtlasError::Source(format!(
                "vote differs from preserved roll call: {}",
                vote.official_vote_id
            )));
        }
        *counts.entry(vote.recorded_vote.as_str()).or_default() += 1;
    }
    if counts.get("Aye") != Some(&6) || counts.get("No") != Some(&3) {
        return Err(AtlasError::Source(
            "roll-call tally differs from case".to_owned(),
        ));
    }
    Ok(())
}

fn source_id(proof: &Proof) -> String {
    match proof.fixture.as_str() {
        "fixtures/co/event-2660-final-vote.json" => "source_co_event_2660".to_owned(),
        "fixtures/co/votes-103550-final-vote.json" => "source_co_votes_103550".to_owned(),
        "fixtures/co/ordinance-25-93-signed.pdf" => "source_co_signed_ordinance_25_93".to_owned(),
        "fixtures/co/police-technology-surcharge-presentation.pdf" => {
            "source_co_staff_presentation".to_owned()
        }
        other => stable_id("atlas-source", &[other]),
    }
}

fn requirements(checked: &Validated) -> Vec<AtlasReviewRequirement> {
    vec![AtlasReviewRequirement {
        id: stable_id("atlas-release", &[&checked.manifest.case_id]),
        bound_digest: review_digest("atlas-release-binding", &[&checked.manifest_digest]),
        kind: "release".to_owned(),
        summary: format!("Release atlas case {}", checked.manifest.case_id),
        source_url: String::new(),
        locator: String::new(),
        source_sha256: checked.manifest_digest.clone(),
    }]
}

fn claim_verification_digest(case_id: &str, claim: &Claim) -> String {
    review_digest(
        "atlas-source-verification",
        &[
            case_id,
            &claim.key,
            &claim.proof.url,
            &claim.proof.sha256,
            &claim.locator,
            &claim.quote,
            &claim.summary,
        ],
    )
}

fn review_digest(namespace: &str, parts: &[&str]) -> String {
    let stable = stable_id(namespace, parts);
    format!(
        "sha256:{}",
        stable.split_once(':').expect("stable id has a digest").1
    )
}

fn production_gate(
    store: &Store,
    checked: &Validated,
    required: &[AtlasReviewRequirement],
    site_url: Option<&str>,
) -> Result<(), AtlasError> {
    if checked.manifest.synthetic {
        return Err(AtlasError::Review(
            "synthetic data cannot be released".to_owned(),
        ));
    }
    if checked.manifest.review_state != "approved" {
        return Err(AtlasError::Review(
            "manifest is pending case-level release review".to_owned(),
        ));
    }
    if !publication_allowlist_allows(store, ATLAS_RELEASE_CATEGORY)
        .map_err(|e| AtlasError::Review(e.to_string()))?
    {
        return Err(AtlasError::Review(
            "atlas_release allowlist is absent".to_owned(),
        ));
    }
    let base =
        site_url.ok_or_else(|| AtlasError::Review("canonical site URL is required".to_owned()))?;
    let parsed = Url::parse(base)
        .map_err(|_| AtlasError::Review("invalid canonical site URL".to_owned()))?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.host_str().is_some_and(|host| {
            host.contains("example") || host == "localhost" || host.ends_with(".invalid")
        })
        || parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(AtlasError::Review(
            "canonical site URL must be a non-placeholder HTTPS origin".to_owned(),
        ));
    }
    let req = required.first().expect("one release requirement");
    let current = store.current_review(&req.id)?;
    let Some(decision) = current else {
        return Err(AtlasError::Review(format!(
            "case lacks release review: {}",
            req.id
        )));
    };
    if decision.state != ReviewState::Approved
        || decision.bound_digest != req.bound_digest
        || !human_reviewer(&decision.reviewer)
    {
        return Err(AtlasError::Review(format!(
            "case lacks current human release approval: {}",
            req.id
        )));
    }
    Ok(())
}

fn human_reviewer(reviewer: &str) -> bool {
    let lower = reviewer.trim().to_ascii_lowercase();
    !lower.is_empty()
        && ![
            "demo",
            "test",
            "fixture",
            "seed",
            "bot",
            "system",
            "auto",
            "synthetic",
        ]
        .iter()
        .any(|part| lower.contains(part))
}

#[allow(clippy::too_many_lines)]
fn snapshot(
    store: &Store,
    checked: &Validated,
    required: &[AtlasReviewRequirement],
    mode: AtlasExportMode,
    site_url: Option<&str>,
) -> Result<Value, AtlasError> {
    let m = &checked.manifest;
    let preview = mode == AtlasExportMode::Preview;
    let decision_id = format!("decision_{}", m.case_id.replace('-', "_"));
    let jurisdiction_id = format!("jurisdiction_{}", m.jurisdiction.id.replace('-', "_"));
    let agency_id = format!("agency_{}", m.agency.id.replace('-', "_"));
    let mut evidence = Vec::new();
    let evidence_state = if preview {
        "PENDING_RELEASE"
    } else {
        "SOURCE_VERIFIED"
    };
    let mut by_key = BTreeMap::<String, (String, String, Value)>::new();
    for claim in &checked.claims {
        let id = format!("evidence_{}", claim.key);
        let review_state = evidence_state.to_owned();
        let corrections = json!([]);
        evidence.push(json!({
            "id":id, "source_id":source_id(&claim.proof), "claim":claim.summary,
            "quote":claim.quote, "locator":claim.locator,
            "authority":"City of Colorado Springs", "retrieved_at":null,
            "sha256":claim.proof.sha256, "review_state":review_state,
            "verification_digest":claim_verification_digest(&m.case_id, claim), "corrections":corrections,
        }));
        by_key.insert(claim.key.clone(), (id, review_state, corrections));
    }
    let release_requirement = required.first().expect("one release requirement");
    let release_history = store.reviews_for_citation(&release_requirement.id)?;
    let release_corrections = release_history
        .iter()
        .filter(|review| review.state != ReviewState::Approved)
        .map(|review| json!({"state":review.state.label(), "at":review.decided_at}))
        .collect::<Vec<_>>();
    let release_review = if preview {
        json!({"state":"PENDING_RELEASE", "bound_digest":release_requirement.bound_digest,
            "reviewed_at":null, "corrections":release_corrections})
    } else {
        let current = release_history
            .last()
            .expect("production gate checked release review");
        json!({"state":"APPROVED", "bound_digest":release_requirement.bound_digest,
            "reviewed_at":current.decided_at, "corrections":release_corrections})
    };
    let link = |key: &str| by_key.get(key).expect("validated claim exists");
    let j = link("jurisdiction");
    let a = link("agency");
    let d = link("decision_action");
    let ordinance = link("decision_ordinance");
    let tech = link("technology_context");
    let mut decision_rels = vec![
        relationship(
            "decision_jurisdiction",
            "WITHIN_JURISDICTION",
            "Colorado Springs jurisdiction",
            &jurisdiction_id,
            j,
        ),
        relationship(
            "decision_agency",
            "NAMES_AGENCY",
            &m.agency.relationship,
            &agency_id,
            a,
        ),
    ];
    let mut votes = Vec::new();
    let mut entities = Vec::new();
    for vote in &m.votes {
        let voter_id = format!("recorded_voter_co_{}", vote.official_person_id);
        let claim_key = format!("vote_{}", vote.official_vote_id);
        let proof = link(&claim_key);
        decision_rels.push(relationship(
            &format!("decision_vote_{}", vote.official_vote_id),
            "RECORDED_VOTE",
            &format!("Recorded {} vote on Ordinance 25-93", vote.recorded_vote),
            &voter_id,
            proof,
        ));
        votes.push(json!({
            "name":vote.person_name, "vote":vote.recorded_vote,
            "voter_entity_id":voter_id,
            "office_role":"not_established", "office_term":"not_established",
            "evidence_id":proof.0, "review_state":proof.1, "corrections":proof.2,
        }));
        entities.push(json!({
            "id":voter_id, "type":"RECORDED_VOTER", "name":vote.person_name,
            "subtitle":"Recorded vote on Ordinance 25-93; office role and term not established",
            "status":proof.1, "description":format!("{} appears in the official roll call with a recorded {} vote.", vote.person_name, vote.recorded_vote),
            "jurisdiction_ids":[jurisdiction_id], "aliases":[], "tags":["public roll call"],
            "updated_at":m.decision.date, "source_count":1, "relation_count":0,
            "attributes":[{"label":"Recorded vote", "value":vote.recorded_vote, "evidence_id":proof.0, "review_state":proof.1, "corrections":proof.2}],
            "relationships":[], "timeline":[], "source_ids":[source_id(&vote.proof)],
            "unknowns":[{"kind":"office_role","state":"not_established"},{"kind":"office_term","state":"not_established"}],
        }));
    }
    for system in &m.technology_context.systems {
        let system_slug = system
            .to_ascii_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>();
        let system_slug = system_slug
            .split('_')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("_");
        let technology_id = format!("technology_co_{system_slug}");
        entities.push(json!({
            "id":technology_id, "type":"TECHNOLOGY", "name":system,
            "subtitle":"Staff cost presentation context; contract link not established",
            "status":tech.1, "description":m.technology_context.relationship,
            "jurisdiction_ids":[jurisdiction_id], "aliases":[], "tags":["presentation context"],
            "updated_at":m.decision.date, "source_count":1, "relation_count":0,
            "attributes":[{"label":"Presentation context", "value":m.technology_context.relationship,
                "evidence_id":tech.0,"review_state":tech.1,"corrections":tech.2}],
            "relationships":[], "timeline":[], "source_ids":[source_id(&m.technology_context.proof)],
            "unknowns":[{"kind":"contract_award","state":"not_established","detail":m.contract_link.detail}],
        }));
    }
    let geo = json!({"lat":38.8339, "lon":-104.8214, "label":"Approximate Colorado Springs map anchor; not an evidenced site location"});
    entities.push(json!({
        "id":jurisdiction_id, "type":"JURISDICTION", "name":m.jurisdiction.name,
        "jurisdiction_ref":"us-co-place-0816000",
        "subtitle":"Colorado municipal jurisdiction", "status":j.1,
        "description":"Jurisdiction of the recorded City Council action",
        "geo":geo, "jurisdiction_ids":[], "aliases":[], "tags":["Colorado"],
        "updated_at":m.decision.date, "source_count":1, "relation_count":0,
        "attributes":[{"label":"Governing body","value":m.decision.body,"evidence_id":j.0,"review_state":j.1,"corrections":j.2}],
        "relationships":[], "timeline":[], "source_ids":[source_id(&m.jurisdiction.proof)],
    }));
    entities.push(json!({
        "id":agency_id, "type":"AGENCY", "name":m.agency.name,
        "subtitle":"Agency named in signed ordinance", "status":a.1,
        "description":m.agency.relationship, "geo":geo,
        "jurisdiction_ids":[jurisdiction_id], "aliases":[], "tags":["police"],
        "updated_at":m.decision.date, "source_count":1, "relation_count":0,
        "attributes":[{"label":"Ordinance reference","value":m.agency.relationship,"evidence_id":a.0,"review_state":a.1,"corrections":a.2}],
        "relationships":[], "timeline":[], "source_ids":[source_id(&m.agency.proof)],
    }));
    let unknown = json!({"kind":"contract_award", "state":"not_established", "detail":m.contract_link.detail});
    let technology = m
        .technology_context
        .systems
        .iter()
        .map(|system| {
            json!({
                "name":system, "context":m.technology_context.relationship,
                "evidence_id":tech.0, "review_state":tech.1, "corrections":tech.2,
            })
        })
        .collect::<Vec<_>>();
    let share_url = if preview {
        None
    } else {
        site_url.map(|base| {
            format!(
                "{}/decisions/{}.html",
                base.trim_end_matches('/'),
                m.case_id
            )
        })
    };
    entities.push(json!({
        "id":decision_id, "slug":m.case_id, "type":"DECISION",
        "name":format!("Ordinance {}", m.decision.ordinance),
        "social_title":"Colorado Springs police technology surcharge vote",
        "social_summary":"Council voted 6–3 on Ordinance 25-93. Read the official vote and signed ordinance. A named vendor contract link is not established by these records.",
        "subtitle":format!("{} • {}–{}", m.decision.date, m.decision.tally.aye, m.decision.tally.no),
        "status":d.1, "description":m.decision.summary, "geo":geo,
        "jurisdiction_ids":[jurisdiction_id], "aliases":[m.decision.official_matter],
        "tags":["technology surcharge", "recorded vote"], "updated_at":m.decision.date,
        "source_count":4, "relation_count":decision_rels.len(),
        "attributes":[
            {"label":"Outcome","value":m.decision.outcome,"evidence_id":d.0,"review_state":d.1,"corrections":d.2},
            {"label":"Vote tally","value":"6 Aye, 3 No","evidence_id":d.0,"review_state":d.1,"corrections":d.2},
            {"label":"Surcharge in ordinance","value":"Police Department Technology Surcharge","evidence_id":ordinance.0,"review_state":ordinance.1,"corrections":ordinance.2}
        ],
        "relationships":decision_rels,
        "timeline":[{"id":"decision_final_passage","date":m.decision.date,"type":"FINAL_PASSAGE","title":"Ordinance 25-93 finally passed, 6–3","evidence_id":d.0,"review_state":d.1,"corrections":d.2}],
        "source_ids":["source_co_event_2660","source_co_votes_103550","source_co_signed_ordinance_25_93","source_co_staff_presentation"],
        "votes":votes, "technology_context":technology,
        "unknowns":[unknown], "unresolved_question":m.unresolved_question,
        "share_url":share_url,
    }));
    entities.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    evidence.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    let relationship_count = entities
        .iter()
        .map(|entity| entity["relationships"].as_array().map_or(0, Vec::len))
        .sum::<usize>();
    Ok(json!({
        "schema_version":"pnull-public-snapshot-v1", "demo":m.synthetic,
        "preview":preview,
        "status":{
            "state":if preview {"pending_release"} else {"released"},
            "dataset_version":checked.manifest_digest, "last_ingest":null,
            "last_publish":null, "manifest_digest":checked.manifest_digest,
            "counts":{"entities":entities.len(),"records":evidence.len(),
                "sources":checked.sources.len(),"relationships":relationship_count},
        },
        "case_releases":[{"case_id":m.case_id,"release":release_review.clone()}],
        "release_review":release_review,
        "activity":[], "entities":entities, "evidence":evidence,
        "sources":checked.sources.values().collect::<Vec<_>>(),
    }))
}

fn relationship(
    id: &str,
    kind: &str,
    label: &str,
    target: &str,
    proof: &(String, String, Value),
) -> Value {
    json!({
        "id":id, "type":kind, "label":label, "target_entity_id":target,
        "source_count":1, "evidence_id":proof.0, "review_state":proof.1,
        "corrections":proof.2,
    })
}

fn validate_value(value: &Value) -> Result<(), AtlasError> {
    match value {
        Value::String(s) => {
            validate_public_text(s).map_err(|e| AtlasError::Privacy(e.to_string()))?;
        }
        Value::Array(items) => {
            for item in items {
                validate_value(item)?;
            }
        }
        Value::Object(fields) => {
            for item in fields.values() {
                validate_value(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pnull_core::{PublicationAllowlist, ReviewDecision};

    fn source_manifest() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/atlas/colorado-springs-25-93.v1.json")
    }

    fn copied_case() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        fs::create_dir_all(root.join("fixtures/atlas")).expect("atlas fixture directory");
        fs::create_dir_all(root.join("fixtures/co")).expect("source fixture directory");
        let manifest = root.join("fixtures/atlas/colorado-springs-25-93.v1.json");
        fs::copy(source_manifest(), &manifest).expect("manifest copy");
        for file in [
            "event-2660-final-vote.json",
            "votes-103550-final-vote.json",
            "ordinance-25-93-signed.pdf",
            "police-technology-surcharge-presentation.pdf",
        ] {
            let original = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/co")
                .join(file);
            fs::copy(original, root.join("fixtures/co").join(file)).expect("source copy");
        }
        (dir, manifest)
    }

    fn edit_manifest(path: &Path, f: impl FnOnce(&mut Value)) {
        let mut manifest: Value = serde_json::from_slice(&fs::read(path).expect("read manifest"))
            .expect("parse manifest");
        f(&mut manifest);
        fs::write(
            path,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&manifest).expect("encode manifest")
            ),
        )
        .expect("write manifest");
    }

    fn approve_release(store: &Store, manifest: &Path, reviewer: &str) {
        let [req] = review_requirements(manifest)
            .expect("one release requirement")
            .try_into()
            .expect("one release requirement");
        let decided_at = "2026-11-09T12:00:00Z".to_owned();
        let decision = ReviewDecision {
            id: ReviewDecision::id_for(&req.id, &decided_at),
            citation_id: req.id,
            state: ReviewState::Approved,
            reviewer: reviewer.to_owned(),
            note: String::new(),
            bound_digest: req.bound_digest,
            decision_digest: String::new(),
            decided_at,
            supersedes: None,
        };
        store.insert_review(&decision).expect("insert review");
    }

    fn allow_release(store: &Store) {
        store
            .insert_publication_allowlist(&PublicationAllowlist {
                id: "atlas-test-release".to_owned(),
                field_categories: vec![ATLAS_RELEASE_CATEGORY.to_owned()],
                created_at: "2026-11-09T12:00:00Z".to_owned(),
                note: String::new(),
            })
            .expect("insert allowlist");
    }

    #[test]
    fn preview_is_deterministic_and_shows_unestablished_links() {
        let (dir, manifest) = copied_case();
        let store = Store::open(dir.path().join("db")).expect("store");
        let one = dir.path().join("one.json");
        let two = dir.path().join("two.json");
        export_atlas(&store, &manifest, &one, AtlasExportMode::Preview, None)
            .expect("first preview");
        export_atlas(&store, &manifest, &two, AtlasExportMode::Preview, None)
            .expect("second preview");
        assert_eq!(fs::read(&one).unwrap(), fs::read(&two).unwrap());
        let data: Value = serde_json::from_slice(&fs::read(one).unwrap()).unwrap();
        assert_eq!(data["preview"], true);
        assert_eq!(data["demo"], false);
        assert_eq!(data["status"]["state"], "pending_release");
        assert_eq!(data["release_review"]["state"], "PENDING_RELEASE");
        assert!(data["release_review"]["reviewed_at"].is_null());
        let decision = data["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["type"] == "DECISION")
            .unwrap();
        assert_eq!(decision["unknowns"][0]["state"], "not_established");
        assert_eq!(decision["votes"].as_array().unwrap().len(), 9);
        assert!(
            decision["relationships"]
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r["type"] != "CONTRACT_APPROVAL")
        );
        assert!(
            data["evidence"]
                .as_array()
                .unwrap()
                .iter()
                .all(|e| e["review_state"] == "PENDING_RELEASE"
                    && e.get("verification_digest").is_some()
                    && e.get("review_bound_digest").is_none())
        );
    }

    #[test]
    fn relative_manifest_path_resolves_preserved_fixtures() {
        let path = Path::new("../../fixtures/atlas/colorado-springs-25-93.v1.json");
        let required = review_requirements(path).expect("relative manifest path");
        assert_eq!(required.len(), 1);
        assert_eq!(required[0].kind, "release");
        assert!(required[0].bound_digest.starts_with("sha256:"));
    }

    #[test]
    fn absent_case_approval_and_synthetic_case_block_production() {
        let (dir, manifest) = copied_case();
        edit_manifest(&manifest, |m| m["review_state"] = json!("pending"));
        let store = Store::open(dir.path().join("db")).expect("store");
        let output = dir.path().join("public.json");
        fs::write(&output, b"existing approved snapshot").unwrap();
        assert!(matches!(
            export_atlas(
                &store,
                &manifest,
                &output,
                AtlasExportMode::Production,
                Some("https://atlas.example.org")
            ),
            Err(AtlasError::Review(_))
        ));
        assert_eq!(fs::read(&output).unwrap(), b"existing approved snapshot");
        allow_release(&store);
        approve_release(&store, &manifest, "Ada Editor");
        assert!(matches!(
            export_atlas(
                &store,
                &manifest,
                &output,
                AtlasExportMode::Production,
                Some("https://atlas.test")
            ),
            Err(AtlasError::Review(_))
        ));
        edit_manifest(&manifest, |m| {
            m["review_state"] = json!("approved");
            m["synthetic"] = json!(true);
        });
        let synthetic_store =
            Store::open(dir.path().join("synthetic-db")).expect("synthetic store");
        allow_release(&synthetic_store);
        approve_release(&synthetic_store, &manifest, "Ada Editor");
        assert!(matches!(
            export_atlas(
                &synthetic_store,
                &manifest,
                &output,
                AtlasExportMode::Production,
                Some("https://atlas.example.org")
            ),
            Err(AtlasError::Review(_))
        ));
        assert_eq!(fs::read(&output).unwrap(), b"existing approved snapshot");
    }

    #[test]
    fn one_case_review_and_allowlist_gate_release() {
        let (dir, manifest) = copied_case();
        edit_manifest(&manifest, |m| m["review_state"] = json!("approved"));
        let store = Store::open(dir.path().join("db")).expect("store");
        let output = dir.path().join("public.json");
        allow_release(&store);
        assert!(matches!(
            export_atlas(
                &store,
                &manifest,
                &output,
                AtlasExportMode::Production,
                Some("https://atlas.test")
            ),
            Err(AtlasError::Review(_))
        ));
        approve_release(&store, &manifest, "Ada Editor");
        let no_allowlist_store = Store::open(dir.path().join("db-no-allowlist")).expect("store");
        approve_release(&no_allowlist_store, &manifest, "Ada Editor");
        assert!(matches!(
            export_atlas(
                &no_allowlist_store,
                &manifest,
                &output,
                AtlasExportMode::Production,
                Some("https://atlas.test")
            ),
            Err(AtlasError::Review(_))
        ));
        export_atlas(
            &store,
            &manifest,
            &output,
            AtlasExportMode::Production,
            Some("https://atlas.test"),
        )
        .expect("reviewed release");
        let data: Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
        assert_eq!(data["preview"], false);
        assert_eq!(data["status"]["state"], "released");
        assert_eq!(data["release_review"]["state"], "APPROVED");
        assert_eq!(
            data["case_releases"][0]["case_id"],
            "co-colorado-springs-25-581-2025-11-25"
        );
        assert_eq!(data["case_releases"][0]["release"], data["release_review"]);
        assert_eq!(
            data["release_review"]["reviewed_at"],
            "2026-11-09T12:00:00Z"
        );
        assert!(
            data["evidence"]
                .as_array()
                .unwrap()
                .iter()
                .all(|e| e["review_state"] == "SOURCE_VERIFIED"
                    && e.get("verification_digest").is_some()
                    && e.get("review_bound_digest").is_none())
        );
        let decision = data["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["type"] == "DECISION")
            .unwrap();
        assert_eq!(
            decision["share_url"],
            "https://atlas.test/decisions/co-colorado-springs-25-581-2025-11-25.html"
        );
        assert_eq!(
            decision["social_title"],
            "Colorado Springs police technology surcharge vote"
        );
    }

    #[test]
    fn superseded_case_review_blocks_release_and_keeps_correction_history() {
        let (dir, manifest) = copied_case();
        edit_manifest(&manifest, |m| m["review_state"] = json!("approved"));
        let store = Store::open(dir.path().join("db")).expect("store");
        allow_release(&store);
        approve_release(&store, &manifest, "Ada Editor");
        let output = dir.path().join("public.json");
        export_atlas(
            &store,
            &manifest,
            &output,
            AtlasExportMode::Production,
            Some("https://atlas.test"),
        )
        .expect("reviewed release");
        let saved = fs::read(&output).unwrap();
        let req = review_requirements(&manifest).unwrap().remove(0);
        let at = "2026-11-09T13:00:00Z".to_owned();
        store
            .insert_review(&ReviewDecision {
                id: ReviewDecision::id_for(&req.id, &at),
                citation_id: req.id,
                state: ReviewState::Superseded,
                reviewer: "Ada Editor".to_owned(),
                note: "private correction note".to_owned(),
                bound_digest: req.bound_digest,
                decision_digest: String::new(),
                decided_at: at,
                supersedes: None,
            })
            .unwrap();
        assert!(matches!(
            export_atlas(
                &store,
                &manifest,
                &output,
                AtlasExportMode::Production,
                Some("https://atlas.test")
            ),
            Err(AtlasError::Review(_))
        ));
        assert_eq!(fs::read(output).unwrap(), saved);
        let private_preview = dir.path().join("corrected-preview.json");
        export_atlas(
            &store,
            &manifest,
            &private_preview,
            AtlasExportMode::Preview,
            None,
        )
        .expect("corrected preview");
        let preview_text = fs::read_to_string(private_preview).unwrap();
        assert!(preview_text.contains("Superseded"));
        assert!(!preview_text.contains("private correction note"));
    }

    #[test]
    fn demo_reviewer_and_stale_release_digest_are_rejected() {
        let (dir, manifest) = copied_case();
        edit_manifest(&manifest, |m| m["review_state"] = json!("approved"));
        let store = Store::open(dir.path().join("db")).unwrap();
        let output = dir.path().join("public.json");
        allow_release(&store);
        approve_release(&store, &manifest, "demo seed");
        assert!(matches!(
            export_atlas(
                &store,
                &manifest,
                &output,
                AtlasExportMode::Production,
                Some("https://atlas.test")
            ),
            Err(AtlasError::Review(_))
        ));
        let store2 = Store::open(dir.path().join("db2")).unwrap();
        allow_release(&store2);
        approve_release(&store2, &manifest, "Ada Editor");
        edit_manifest(&manifest, |m| {
            m["unresolved_question"] = json!("Which contracts were later paid?");
        });
        assert!(matches!(
            export_atlas(
                &store2,
                &manifest,
                &output,
                AtlasExportMode::Production,
                Some("https://atlas.test")
            ),
            Err(AtlasError::Review(_))
        ));
    }

    #[test]
    fn source_digest_and_public_private_data_are_rejected() {
        let (dir, manifest) = copied_case();
        edit_manifest(&manifest, |m| m["review_state"] = json!("approved"));
        let store = Store::open(dir.path().join("approved-db")).expect("store");
        allow_release(&store);
        approve_release(&store, &manifest, "Ada Editor");
        let output = dir.path().join("public.json");
        fs::write(&output, b"previous published bytes").expect("sentinel");
        let fixture = dir.path().join("fixtures/co/votes-103550-final-vote.json");
        fs::write(&fixture, b"changed raw bytes").unwrap();
        assert!(matches!(
            review_requirements(&manifest),
            Err(AtlasError::Source(_))
        ));
        assert!(matches!(
            export_atlas(
                &store,
                &manifest,
                &output,
                AtlasExportMode::Production,
                Some("https://atlas.test")
            ),
            Err(AtlasError::Source(_))
        ));
        assert_eq!(fs::read(&output).unwrap(), b"previous published bytes");
        let (_dir2, manifest2) = copied_case();
        edit_manifest(&manifest2, |m| {
            m["unresolved_question"] = json!("Call resident@example.com about this decision");
        });
        let private_store_dir = tempfile::tempdir().unwrap();
        let private_store = Store::open(private_store_dir.path()).unwrap();
        let output = manifest2.with_extension("preview.json");
        assert!(matches!(
            export_atlas(
                &private_store,
                &manifest2,
                &output,
                AtlasExportMode::Preview,
                None
            ),
            Err(AtlasError::Privacy(_))
        ));
        assert!(!output.exists());
        let (_dir3, manifest3) = copied_case();
        edit_manifest(&manifest3, |m| {
            m["votes"][0]["proof"]["name_locator"] = json!("$[1].VotePersonName");
        });
        assert!(matches!(
            review_requirements(&manifest3),
            Err(AtlasError::Source(_))
        ));
        let (_dir4, manifest4) = copied_case();
        edit_manifest(&manifest4, |m| {
            m["contract_link"]["state"] = json!("approved");
        });
        assert!(matches!(
            review_requirements(&manifest4),
            Err(AtlasError::Manifest(_))
        ));
    }

    #[test]
    fn proof_cannot_point_to_an_unrelated_official_page() {
        let (_dir, manifest) = copied_case();
        edit_manifest(&manifest, |m| {
            m["decision"]["proofs"][0]["url"] =
                json!("https://webapi.legistar.com/v1/coloradosprings/events/1");
        });
        assert!(matches!(
            review_requirements(&manifest),
            Err(AtlasError::Source(_))
        ));
    }

    #[test]
    fn pdf_proof_cannot_claim_an_unchecked_quote_or_wrong_document() {
        let (_dir, manifest) = copied_case();
        edit_manifest(&manifest, |m| {
            m["agency"]["proof"]["quote"] = json!("City Council approved a Flock contract");
        });
        assert!(matches!(
            review_requirements(&manifest),
            Err(AtlasError::Source(_))
        ));
        let (_dir, manifest) = copied_case();
        edit_manifest(&manifest, |m| {
            m["decision"]["proofs"][1] = m["technology_context"]["proof"].clone();
        });
        assert!(matches!(
            review_requirements(&manifest),
            Err(AtlasError::Source(_))
        ));
    }

    #[test]
    fn unchecked_technology_or_agency_cannot_be_marked_source_verified() {
        let (_dir, manifest) = copied_case();
        edit_manifest(&manifest, |m| {
            m["technology_context"]["systems"]
                .as_array_mut()
                .unwrap()
                .push(json!("Face Recognition Network (fictional)"));
        });
        assert!(matches!(
            review_requirements(&manifest),
            Err(AtlasError::Manifest(_) | AtlasError::Source(_))
        ));
        let (_dir, manifest) = copied_case();
        edit_manifest(&manifest, |m| {
            m["agency"]["name"] = json!("Fictional Police Department");
        });
        assert!(matches!(
            review_requirements(&manifest),
            Err(AtlasError::Manifest(_) | AtlasError::Source(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn preexisting_temporary_symlink_cannot_clobber_other_file() {
        use std::os::unix::fs::symlink;

        let (dir, manifest) = copied_case();
        let store = Store::open(dir.path().join("db")).unwrap();
        let output = dir.path().join("atlas.json");
        let sentinel = dir.path().join("unrelated.txt");
        fs::write(&sentinel, b"unrelated contents").unwrap();
        symlink(&sentinel, output.with_extension("json.pnull-building")).unwrap();
        export_atlas(&store, &manifest, &output, AtlasExportMode::Preview, None).unwrap();
        assert_eq!(fs::read(&sentinel).unwrap(), b"unrelated contents");
        let data: Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
        assert_eq!(data["preview"], true);
    }

    #[test]
    fn event_and_vote_locators_must_refer_to_the_same_checked_record() {
        let (_dir, manifest) = copied_case();
        edit_manifest(&manifest, |m| {
            m["decision"]["proofs"][0]["locators"][0] = json!("$.EventItems[37].EventItemId");
        });
        assert!(matches!(
            review_requirements(&manifest),
            Err(AtlasError::Source(_))
        ));

        let (_dir, manifest) = copied_case();
        edit_manifest(&manifest, |m| {
            m["votes"][0]["proof"]["vote_locator"] = json!("$[2].VoteValueName");
        });
        assert!(matches!(
            review_requirements(&manifest),
            Err(AtlasError::Source(_))
        ));
    }

    #[test]
    fn canonical_url_cannot_contain_credentials() {
        let (dir, manifest) = copied_case();
        let store = Store::open(dir.path().join("db")).unwrap();
        allow_release(&store);
        approve_release(&store, &manifest, "Ada Editor");
        let output = dir.path().join("public.json");
        assert!(matches!(
            export_atlas(
                &store,
                &manifest,
                &output,
                AtlasExportMode::Production,
                Some("https://editor:password@panopticon.fail/")
            ),
            Err(AtlasError::Review(_))
        ));
        assert!(!output.exists());
    }

    #[test]
    fn oversized_fixture_is_rejected_without_reading_unbounded_input() {
        let (dir, manifest) = copied_case();
        let fixture = dir.path().join("fixtures/co/votes-103550-final-vote.json");
        fs::OpenOptions::new()
            .write(true)
            .open(&fixture)
            .unwrap()
            .set_len(MAX_FIXTURE_BYTES + 1)
            .unwrap();
        assert!(matches!(
            review_requirements(&manifest),
            Err(AtlasError::Source(message)) if message.contains("byte limit")
        ));
    }

    #[test]
    fn verified_proposal_append_updates_bundle_and_rejects_duplicate_case() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut base: Value = serde_json::from_slice(
            &fs::read(root.join("web/panopticon-terminal/public/public-snapshot.json"))
                .expect("checked-in release"),
        )
        .expect("release JSON");
        let case_id = "co-record-test-proposal";
        let extra = json!({
            "schema_version":"pnull-public-snapshot-v1","demo":false,"preview":false,
            "status":{"state":"released"},
            "case_releases":[{"case_id":case_id,"release":{
                "state":"SOURCE_VERIFIED","bound_digest":format!("sha256:{}", "c".repeat(64)),
                "reviewed_at":null,"corrections":[]}}],
            "entities":[{"id":"decision_proposal_test","type":"DECISION","slug":case_id,
                "name":"Test proposal","source_count":0,"relation_count":0,"source_ids":[],
                "relationships":[],"attributes":[],"timeline":[],"votes":[]}],
            "evidence":[],"sources":[]
        });
        append_verified_proposal_snapshot(&mut base, &extra).expect("append");
        assert_eq!(base["case_releases"].as_array().expect("releases").len(), 5);
        assert_eq!(base["status"]["counts"]["entities"], 45);
        assert!(
            base["entities"]
                .as_array()
                .expect("entities")
                .iter()
                .any(|item| item["slug"] == case_id)
        );
        assert!(append_verified_proposal_snapshot(&mut base, &extra).is_err());
        if let Ok(path) = std::env::var("PNUL_PROPOSAL_BUNDLE_TEST_OUTPUT") {
            fs::write(path, serde_json::to_vec_pretty(&base).expect("JSON")).expect("QA bundle");
        }
    }
}
