//! Version 2 atlas cases are assembled from independently preserved official
//! sources. The registry pins source identity; every published field is bound
//! to an exact locator and quote in those bytes. A case has one release review.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::net::IpAddr;
use std::path::{Component, Path};

use pnull_core::{ReviewState, Store, sha256_hex, stable_id};
use pnull_http::is_public_address;
use scraper::{Html, Selector};
use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use url::Url;

use super::{
    ATLAS_RELEASE_CATEGORY, AtlasError, AtlasExportMode, AtlasReviewRequirement, MAX_FIXTURE_BYTES,
    human_reviewer, locate, review_digest,
};
use crate::publication_allowlist_allows;

const REGISTRY_PATH: &str = "fixtures/atlas/sources.co.v1.json";
const GEOGRAPHY_PATH: &str = "web/panopticon-terminal/geography/colorado-jurisdictions.json";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceRegistry {
    schema_version: u32,
    sources: Vec<RegisteredSource>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegisteredSource {
    id: String,
    #[serde(default)]
    acquisition_source_id: Option<String>,
    #[serde(default)]
    acquisition_config_digest: Option<String>,
    authority: String,
    url: String,
    fixture: String,
    sha256: String,
    title: String,
    format: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fact {
    value: String,
    proof: ProofRef,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProofRef {
    source_id: String,
    locator: String,
    quote: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Place {
    id: String,
    jurisdiction_ref: String,
    name: Fact,
    #[serde(default)]
    display_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Agency {
    id: String,
    name: Fact,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Decision {
    official_matter: Fact,
    event_item_id: Fact,
    title: Fact,
    #[serde(default)]
    short_label: Option<Fact>,
    #[serde(default)]
    display_title: Option<String>,
    date: Fact,
    #[serde(default)]
    body: Option<Fact>,
    #[serde(default)]
    action: Option<Fact>,
    outcome: Fact,
    outcome_class: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_field_names)]
struct Vote {
    person_name: Fact,
    recorded_vote: Fact,
    official_vote_id: Fact,
    official_person_id: Fact,
    decision_ref: Fact,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConsentContext {
    item_consent: Fact,
    block_item_id: Fact,
    block_title: Fact,
    block_action: Fact,
    block_outcome: Fact,
    block_vote_source_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TechnologyContext {
    name: Fact,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AgreementContext {
    name: Fact,
    #[serde(default)]
    display_name: Option<String>,
    status: String,
    #[serde(default)]
    proposed_total: Option<Fact>,
    #[serde(default)]
    proposed_increase: Option<Fact>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MissingLink {
    state: String,
    detail: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    case_id: String,
    synthetic: bool,
    review_state: String,
    source_registry: String,
    jurisdiction: Place,
    #[serde(default)]
    agency: Option<Agency>,
    decision: Decision,
    vote_record_state: String,
    #[serde(default)]
    votes: Vec<Vote>,
    #[serde(default)]
    consent_context: Option<ConsentContext>,
    contract_link: MissingLink,
    #[serde(default)]
    technology_context: Vec<TechnologyContext>,
    #[serde(default)]
    agreement_context: Option<AgreementContext>,
    #[serde(default)]
    supporting_sources: Vec<String>,
    unresolved_question: String,
}

struct VerifiedSource {
    registered: RegisteredSource,
    bytes: Vec<u8>,
}

pub(super) struct Checked {
    manifest: Manifest,
    binding_digest: String,
    sources: BTreeMap<String, VerifiedSource>,
    facts: BTreeMap<String, Fact>,
}

#[allow(clippy::too_many_lines)]
pub(super) fn inspect(path: &Path) -> Result<Checked, AtlasError> {
    let manifest_path = path.canonicalize()?;
    let manifest_raw = fs::read(&manifest_path)?;
    let manifest_digest = sha256_hex(&manifest_raw);
    let manifest: Manifest = serde_json::from_slice(&manifest_raw)?;
    if manifest.schema_version != 2 || manifest.source_registry != REGISTRY_PATH {
        return Err(AtlasError::Manifest(
            "unsupported v2 manifest or source registry".to_owned(),
        ));
    }
    check_slug(&manifest.case_id, "case id")?;
    if !manifest.case_id.starts_with("co-") {
        return Err(AtlasError::Manifest(
            "v2 case is outside Colorado".to_owned(),
        ));
    }
    check_slug(&manifest.jurisdiction.id, "jurisdiction id")?;
    if !manifest.jurisdiction.id.starts_with("co-") {
        return Err(AtlasError::Manifest(
            "jurisdiction is outside Colorado".to_owned(),
        ));
    }
    if let Some(agency) = &manifest.agency {
        check_slug(&agency.id, "agency id")?;
    }
    if !matches!(
        manifest.review_state.as_str(),
        "pending" | "approved" | "source_verified"
    ) {
        return Err(AtlasError::Manifest("invalid case review state".to_owned()));
    }
    if manifest.contract_link.state != "not_established"
        || manifest.contract_link.detail.trim().is_empty()
        || manifest.unresolved_question.trim().is_empty()
    {
        return Err(AtlasError::Manifest(
            "contract link must be not_established with a specific unresolved question".to_owned(),
        ));
    }
    if let Some(display) = &manifest.jurisdiction.display_name
        && !display.eq_ignore_ascii_case(&manifest.jurisdiction.name.value)
    {
        return Err(AtlasError::Manifest(
            "jurisdiction display name differs from source-backed name".to_owned(),
        ));
    }
    let registry_path = registry_path(&manifest_path)?;
    let registry_raw = fs::read(&registry_path)?;
    let registry_digest = sha256_hex(&registry_raw);
    let registry: SourceRegistry = serde_json::from_slice(&registry_raw)?;
    if registry.schema_version != 1 || registry.sources.is_empty() {
        return Err(AtlasError::Source(
            "invalid Colorado source registry".to_owned(),
        ));
    }
    let geography_raw = fs::read(geography_path(&manifest_path)?)?;
    let geography_digest = sha256_hex(&geography_raw);
    verify_jurisdiction(&manifest.jurisdiction, &geography_raw)?;
    let mut registered = BTreeMap::new();
    for source in registry.sources {
        check_slug(&source.id, "source id")?;
        if registered.insert(source.id.clone(), source).is_some() {
            return Err(AtlasError::Source(
                "duplicate registry source id".to_owned(),
            ));
        }
    }
    let facts = collect_facts(&manifest)?;
    let mut sources = BTreeMap::new();
    for (key, fact) in &facts {
        if fact.value.trim().is_empty() || fact.proof.quote.trim().is_empty() {
            return Err(AtlasError::Source(format!("empty claim or quote at {key}")));
        }
        let value_bound = if key == "decision_date" {
            fact.proof.quote == fact.value
                || fact.proof.quote.starts_with(&format!("{}T", fact.value))
        } else {
            fact.proof.quote == fact.value
        };
        if !value_bound {
            return Err(AtlasError::Source(format!(
                "claim differs from exact source quote at {key}"
            )));
        }
        let source = registered
            .get(&fact.proof.source_id)
            .ok_or_else(|| AtlasError::Source(format!("unregistered source at {key}")))?;
        if !sources.contains_key(&source.id) {
            let bytes = read_source(&manifest_path, source)?;
            sources.insert(
                source.id.clone(),
                VerifiedSource {
                    registered: source.clone(),
                    bytes,
                },
            );
        }
        verify_fact(
            key,
            fact,
            sources.get(&source.id).expect("source just loaded"),
        )?;
    }
    for source_id in &manifest.supporting_sources {
        let source = registered.get(source_id).ok_or_else(|| {
            AtlasError::Source(format!("unregistered supporting source: {source_id}"))
        })?;
        if !sources.contains_key(source_id) {
            let bytes = read_source(&manifest_path, source)?;
            sources.insert(
                source_id.clone(),
                VerifiedSource {
                    registered: source.clone(),
                    bytes,
                },
            );
        }
    }
    verify_relationships(&manifest, &sources)?;
    let binding_digest = review_digest(
        "atlas-v2-release-binding",
        &[&manifest_digest, &registry_digest, &geography_digest],
    );
    Ok(Checked {
        manifest,
        binding_digest,
        sources,
        facts,
    })
}

fn check_slug(value: &str, label: &str) -> Result<(), AtlasError> {
    if value.is_empty()
        || value.len() > 120
        || !value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        || value.starts_with('-')
        || value.ends_with('-')
    {
        return Err(AtlasError::Manifest(format!("invalid {label}: {value}")));
    }
    Ok(())
}

fn registry_path(manifest_path: &Path) -> Result<std::path::PathBuf, AtlasError> {
    repository_file(manifest_path, REGISTRY_PATH)
}

fn geography_path(manifest_path: &Path) -> Result<std::path::PathBuf, AtlasError> {
    repository_file(manifest_path, GEOGRAPHY_PATH)
}

fn repository_file(manifest_path: &Path, relative: &str) -> Result<std::path::PathBuf, AtlasError> {
    let prefix = if relative.starts_with("fixtures/") {
        "fixtures"
    } else {
        "web"
    };
    for ancestor in manifest_path.ancestors().skip(1) {
        let candidate = ancestor.join(relative);
        if candidate.is_file() {
            let root = ancestor.canonicalize()?;
            let resolved = candidate.canonicalize()?;
            if resolved.starts_with(root.join(prefix)) {
                return Ok(resolved);
            }
        }
    }
    Err(AtlasError::Source(format!(
        "repository file unavailable: {relative}"
    )))
}

fn normalize_place_name(name: &str, kind: &str, roster_name: bool) -> String {
    let mut words = name
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if kind == "municipality" {
        if words.starts_with(&[
            "city".to_owned(),
            "and".to_owned(),
            "county".to_owned(),
            "of".to_owned(),
        ]) {
            words.drain(..4);
        } else if words.len() >= 2
            && matches!(words[0].as_str(), "city" | "town")
            && words[1] == "of"
        {
            words.drain(..2);
        }
        if words
            .last()
            .is_some_and(|word| matches!(word.as_str(), "colorado" | "co"))
        {
            words.pop();
        }
        if roster_name
            && words
                .last()
                .is_some_and(|word| matches!(word.as_str(), "city" | "town"))
        {
            words.pop();
        }
    } else if kind == "county" {
        if words.starts_with(&["county".to_owned(), "of".to_owned()]) {
            words.drain(..2);
        }
        if words
            .last()
            .is_some_and(|word| matches!(word.as_str(), "colorado" | "co"))
        {
            words.pop();
        }
        if words.last().is_some_and(|word| word == "county") {
            words.pop();
        }
    }
    words.join(" ")
}

fn verify_jurisdiction(place: &Place, raw: &[u8]) -> Result<(), AtlasError> {
    let roster: Value = serde_json::from_slice(raw)?;
    if roster["schema_version"] != 1 {
        return Err(AtlasError::Source(
            "unsupported geography roster".to_owned(),
        ));
    }
    let entries = roster["jurisdictions"]
        .as_array()
        .ok_or_else(|| AtlasError::Source("geography roster lacks jurisdictions".to_owned()))?;
    let entry = entries
        .iter()
        .find(|entry| entry["id"] == place.jurisdiction_ref)
        .ok_or_else(|| {
            AtlasError::Source(format!(
                "jurisdiction_ref is absent from archived geography: {}",
                place.jurisdiction_ref
            ))
        })?;
    let kind = entry["kind"]
        .as_str()
        .ok_or_else(|| AtlasError::Source("geography entry lacks kind".to_owned()))?;
    let roster_name = entry["name"]
        .as_str()
        .ok_or_else(|| AtlasError::Source("geography entry lacks name".to_owned()))?;
    let source_name = place.display_name.as_deref().unwrap_or(&place.name.value);
    let normalized = normalize_place_name(source_name, kind, false);
    if normalized != normalize_place_name(roster_name, kind, true) {
        return Err(AtlasError::Source(format!(
            "jurisdiction_ref does not match source-backed name: {}",
            place.jurisdiction_ref
        )));
    }
    let matching_names = entries
        .iter()
        .filter(|entry| {
            entry["kind"] == kind
                && entry["name"]
                    .as_str()
                    .is_some_and(|name| normalize_place_name(name, kind, true) == normalized)
        })
        .count();
    if matching_names != 1 {
        return Err(AtlasError::Source(format!(
            "jurisdiction name is ambiguous in archived geography: {source_name}"
        )));
    }
    Ok(())
}

fn read_source(manifest_path: &Path, source: &RegisteredSource) -> Result<Vec<u8>, AtlasError> {
    if source.title.trim().is_empty() || source.authority.trim().is_empty() {
        return Err(AtlasError::Source(
            "source metadata is incomplete".to_owned(),
        ));
    }
    if !matches!(source.format.as_str(), "json" | "html" | "pdf" | "docx") {
        return Err(AtlasError::Source(format!(
            "v2 source format cannot be quote-verified: {}",
            source.format
        )));
    }
    let url = Url::parse(&source.url)
        .map_err(|error| AtlasError::Source(format!("invalid official URL: {error}")))?;
    if !official_source_url(&url) {
        return Err(AtlasError::Source(format!(
            "untrusted official source URL: {}",
            source.url
        )));
    }
    let rel = Path::new(&source.fixture);
    if rel.is_absolute()
        || ![
            "fixtures/co/",
            "fixtures/co2/",
            "fixtures/co-commerce/",
            "fixtures/co-castlerock/",
            "fixtures/co-denver/",
            "fixtures/co-arapahoe/",
        ]
        .iter()
        .any(|prefix| source.fixture.starts_with(prefix))
        || rel
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || rel.extension().and_then(|ext| ext.to_str()) != Some(source.format.as_str())
    {
        return Err(AtlasError::Source(format!(
            "unsafe source fixture: {}",
            source.fixture
        )));
    }
    let path = source_fixture_path(manifest_path, &source.fixture)?;
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_FIXTURE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FIXTURE_BYTES {
        return Err(AtlasError::Source("source exceeds byte limit".to_owned()));
    }
    if source.sha256.len() != 64
        || !source.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        || sha256_hex(&bytes) != source.sha256
    {
        return Err(AtlasError::Source(format!(
            "source digest mismatch: {}",
            source.id
        )));
    }
    Ok(bytes)
}

fn official_source_url(url: &Url) -> bool {
    if url.scheme() != "https"
        || url.username() != ""
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
    {
        return false;
    }
    let path = url.path().to_ascii_lowercase();
    let extension = Path::new(&path)
        .extension()
        .and_then(|value| value.to_str());
    match url.host_str() {
        Some("webapi.legistar.com") => [
            "coloradosprings",
            "commerce",
            "denver",
            "castlerock-co",
            "arapahoe",
        ]
        .iter()
        .any(|city| path.starts_with(&format!("/v1/{city}/"))),
        Some(
            "coloradosprings.legistar.com"
            | "commerce.legistar.com"
            | "denver.legistar.com"
            | "castlerock-co.legistar.com"
            | "arapahoe.legistar.com",
        ) => path == "/legislationdetail.aspx" || path == "/view.ashx",
        Some("legistar.granicus.com") => {
            ["commerce", "castlerock-co", "denver"]
                .iter()
                .any(|city| path.starts_with(&format!("/{city}/attachments/")))
                && extension == Some("pdf")
        }
        Some("legistar2.granicus.com") => {
            path.starts_with("/castlerock-co/attachments/")
                && matches!(extension, Some("pdf" | "docx"))
        }
        Some("denver.legistar1.com") => {
            path.starts_with("/denver/attachments/") && extension == Some("pdf")
        }
        Some("arapahoe.legistar1.com") => {
            (path.starts_with("/arapahoe/attachments/") || path.starts_with("/arapahoe/meetings/"))
                && extension == Some("pdf")
        }
        _ => false,
    }
}

fn source_fixture_path(
    manifest_path: &Path,
    relative: &str,
) -> Result<std::path::PathBuf, AtlasError> {
    for ancestor in manifest_path.ancestors().skip(1) {
        let candidate = ancestor.join(relative);
        if candidate.is_file() {
            let root = ancestor.canonicalize()?;
            let resolved = candidate.canonicalize()?;
            if resolved.starts_with(root.join("fixtures")) {
                return Ok(resolved);
            }
        }
    }
    Err(AtlasError::Source(format!(
        "fixture unavailable: {relative}"
    )))
}

fn verify_fact(key: &str, fact: &Fact, source: &VerifiedSource) -> Result<(), AtlasError> {
    let actual = match source.registered.format.as_str() {
        "json" => {
            let value: Value = serde_json::from_slice(&source.bytes)?;
            let located = locate(&value, &fact.proof.locator).ok_or_else(|| {
                AtlasError::Source(format!("missing locator {}", fact.proof.locator))
            })?;
            match located {
                Value::String(text) => text.clone(),
                Value::Number(number) => number.to_string(),
                Value::Bool(boolean) => boolean.to_string(),
                _ => {
                    return Err(AtlasError::Source(format!(
                        "locator is not a scalar: {}",
                        fact.proof.locator
                    )));
                }
            }
        }
        "html" => {
            let selector = Selector::parse(&fact.proof.locator).map_err(|_| {
                AtlasError::Source(format!("invalid CSS locator: {}", fact.proof.locator))
            })?;
            let text = std::str::from_utf8(&source.bytes)
                .map_err(|_| AtlasError::Source("HTML source is not UTF-8".to_owned()))?;
            let page = Html::parse_document(text);
            let mut nodes = page.select(&selector);
            let one = nodes.next().ok_or_else(|| {
                AtlasError::Source(format!("missing CSS locator: {}", fact.proof.locator))
            })?;
            if nodes.next().is_some() {
                return Err(AtlasError::Source(format!(
                    "ambiguous CSS locator: {}",
                    fact.proof.locator
                )));
            }
            normalized_whitespace(&one.text().collect::<Vec<_>>().join(" "))
        }
        _ => {
            return Err(AtlasError::Source(
                "binary supporting document cannot prove a claim automatically".to_owned(),
            ));
        }
    };
    let excerpt = key == "jurisdiction"
        || key == "decision_short_label"
        || key == "agreement"
        || key.starts_with("agreement_proposed_")
        || key.starts_with("technology_");
    let quoted = if source.registered.format == "json" && excerpt {
        actual.contains(&fact.proof.quote)
    } else {
        actual == fact.proof.quote
    };
    if !quoted {
        return Err(AtlasError::Source(format!(
            "quote differs at {} in {}",
            fact.proof.locator, fact.proof.source_id
        )));
    }
    Ok(())
}

fn normalized_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn collect_facts(manifest: &Manifest) -> Result<BTreeMap<String, Fact>, AtlasError> {
    let mut facts = BTreeMap::new();
    let mut insert = |key: String, fact: &Fact| {
        facts.insert(key, fact.clone());
    };
    insert("jurisdiction".to_owned(), &manifest.jurisdiction.name);
    if let Some(agency) = &manifest.agency {
        insert("agency".to_owned(), &agency.name);
    }
    let decision = &manifest.decision;
    for (key, fact) in [
        ("matter", &decision.official_matter),
        ("event_item", &decision.event_item_id),
        ("title", &decision.title),
        ("date", &decision.date),
        ("outcome", &decision.outcome),
    ] {
        insert(format!("decision_{key}"), fact);
    }
    if let Some(body) = &decision.body {
        insert("decision_body".to_owned(), body);
    }
    if let Some(action) = &decision.action {
        insert("decision_action".to_owned(), action);
    }
    if let Some(short) = &decision.short_label {
        insert("decision_short_label".to_owned(), short);
    }
    for (index, vote) in manifest.votes.iter().enumerate() {
        for (key, fact) in [
            ("name", &vote.person_name),
            ("vote", &vote.recorded_vote),
            ("vote_id", &vote.official_vote_id),
            ("person_id", &vote.official_person_id),
            ("decision_ref", &vote.decision_ref),
        ] {
            insert(format!("vote_{index}_{key}"), fact);
        }
    }
    if let Some(consent) = &manifest.consent_context {
        for (key, fact) in [
            ("item_consent", &consent.item_consent),
            ("block_item_id", &consent.block_item_id),
            ("block_title", &consent.block_title),
            ("block_action", &consent.block_action),
            ("block_outcome", &consent.block_outcome),
        ] {
            insert(format!("consent_{key}"), fact);
        }
    }
    for (index, context) in manifest.technology_context.iter().enumerate() {
        insert(format!("technology_{index}"), &context.name);
    }
    if let Some(agreement) = &manifest.agreement_context {
        insert("agreement".to_owned(), &agreement.name);
        if let Some(total) = &agreement.proposed_total {
            insert("agreement_proposed_total".to_owned(), total);
        }
        if let Some(increase) = &agreement.proposed_increase {
            insert("agreement_proposed_increase".to_owned(), increase);
        }
    }
    if facts.len() > 512 {
        return Err(AtlasError::Manifest("too many case facts".to_owned()));
    }
    Ok(facts)
}

#[allow(clippy::too_many_lines)]
fn verify_relationships(
    manifest: &Manifest,
    sources: &BTreeMap<String, VerifiedSource>,
) -> Result<(), AtlasError> {
    let decision = &manifest.decision;
    let direct_votes = manifest.vote_record_state == "direct"
        && !manifest.votes.is_empty()
        && manifest.consent_context.is_none();
    let consent_block = manifest.vote_record_state == "consent_block"
        && manifest.votes.is_empty()
        && manifest.consent_context.is_some();
    if !direct_votes && !consent_block {
        return Err(AtlasError::Manifest(
            "v2 case must have a complete direct roll call or verified consent block".to_owned(),
        ));
    }
    let event_source_id = &decision.event_item_id.proof.source_id;
    let event_source = &sources[event_source_id];
    if event_source.registered.format != "json" {
        return Err(AtlasError::Source(
            "automatic decision verification requires official JSON".to_owned(),
        ));
    }
    let item_prefix = decision
        .event_item_id
        .proof
        .locator
        .strip_suffix(".EventItemId")
        .ok_or_else(|| AtlasError::Source("event item locator is not an ID".to_owned()))?;
    if !item_prefix.starts_with("$.EventItems[") || !item_prefix.ends_with(']') {
        return Err(AtlasError::Source(
            "decision is not a Legistar event item".to_owned(),
        ));
    }
    for (fact, field) in [
        (&decision.official_matter, "EventItemMatterFile"),
        (&decision.title, "EventItemTitle"),
        (&decision.outcome, "EventItemPassedFlagName"),
    ] {
        if fact.proof.source_id != *event_source_id
            || fact.proof.locator != format!("{item_prefix}.{field}")
        {
            return Err(AtlasError::Source(format!(
                "decision {field} is not bound to the same event item"
            )));
        }
    }
    if decision.date.proof.source_id != *event_source_id
        || decision.date.proof.locator != "$.EventDate"
    {
        return Err(AtlasError::Source(
            "decision date is not bound to event".to_owned(),
        ));
    }
    if let Some(body) = &decision.body
        && (body.proof.source_id != *event_source_id || body.proof.locator != "$.EventBodyName")
    {
        return Err(AtlasError::Source(
            "decision body is not bound to event".to_owned(),
        ));
    }
    if let Some(action) = &decision.action
        && (action.proof.source_id != *event_source_id
            || !["EventItemActionName", "EventItemActionText"]
                .iter()
                .any(|field| action.proof.locator == format!("{item_prefix}.{field}")))
    {
        return Err(AtlasError::Source(
            "decision action is not bound to event item".to_owned(),
        ));
    }
    if let Some(short) = &decision.short_label
        && (short.proof.source_id != *event_source_id
            || short.proof.locator != decision.title.proof.locator)
    {
        return Err(AtlasError::Source(
            "short label is not in decision title".to_owned(),
        ));
    }
    let generated_title = decision.short_label.as_ref().map_or_else(
        || decision.title.value.clone(),
        |short| format!("{}: {}", decision.official_matter.value, short.value),
    );
    if let Some(display) = &decision.display_title
        && !display.eq_ignore_ascii_case(&generated_title)
    {
        return Err(AtlasError::Manifest(
            "display title differs from source-backed label".to_owned(),
        ));
    }
    if manifest.jurisdiction.name.proof.source_id != *event_source_id {
        return Err(AtlasError::Source(
            "jurisdiction cites a different event".to_owned(),
        ));
    }
    if let Some(agency) = &manifest.agency
        && (agency.name.proof.source_id != *event_source_id
            || agency.name.proof.locator != decision.title.proof.locator)
    {
        return Err(AtlasError::Source(
            "agency is not named in the same decision title".to_owned(),
        ));
    }
    for context in &manifest.technology_context {
        if context.name.proof.source_id != *event_source_id
            || context.name.proof.locator != decision.title.proof.locator
        {
            return Err(AtlasError::Source(
                "technology context is not named in decision title".to_owned(),
            ));
        }
    }
    if let Some(agreement) = &manifest.agreement_context {
        let title = decision.title.value.to_ascii_lowercase();
        let named_agreement = agreement.name.value.to_ascii_lowercase();
        let approval_phrase = [
            "approving ",
            "approving a ",
            "approving an ",
            "approved ",
            "approved a ",
            "approved an ",
        ]
        .iter()
        .any(|prefix| title.contains(&format!("{prefix}{named_agreement}")));
        if decision.outcome_class != "approved"
            || agreement.name.proof.source_id != *event_source_id
            || agreement.name.proof.locator != decision.title.proof.locator
            || !matches!(
                agreement.status.as_str(),
                "approved_format" | "award_authorized" | "proposal_approved"
            )
            || !approval_phrase
            || (agreement.status == "approved_format" && !title.contains("format attached"))
            || (agreement.status == "award_authorized" && !title.contains("sole source"))
            || (agreement.status == "proposal_approved" && !title.contains("proposed "))
        {
            return Err(AtlasError::Source(
                "agreement context is not supported by an approved decision title".to_owned(),
            ));
        }
        if let Some(display) = &agreement.display_name
            && !display.eq_ignore_ascii_case(&agreement.name.value)
        {
            return Err(AtlasError::Manifest(
                "agreement display name differs from source-backed name".to_owned(),
            ));
        }
        for (amount, required_phrases) in [
            (&agreement.proposed_total, &["new total of ", "for "][..]),
            (&agreement.proposed_increase, &["add "][..]),
        ] {
            if let Some(amount) = amount {
                let value = amount.value.to_ascii_lowercase();
                if agreement.status != "proposal_approved"
                    || amount.proof.source_id != *event_source_id
                    || amount.proof.locator != decision.title.proof.locator
                    || !value.starts_with('$')
                    || value.len() > 32
                    || !required_phrases
                        .iter()
                        .any(|phrase| title.contains(&format!("{phrase}{value}")))
                {
                    return Err(AtlasError::Source(
                        "proposed amount is not labeled in approved decision title".to_owned(),
                    ));
                }
            }
        }
    }
    let event: Value = serde_json::from_slice(&event_source.bytes)?;
    let event_id = event["EventId"]
        .as_u64()
        .ok_or_else(|| AtlasError::Source("event lacks numeric EventId".to_owned()))?;
    let item = locate(&event, item_prefix)
        .ok_or_else(|| AtlasError::Source("event item disappeared".to_owned()))?;
    let event_url = Url::parse(&event_source.registered.url)
        .map_err(|_| AtlasError::Source("invalid event source URL".to_owned()))?;
    let event_parts = event_url
        .path_segments()
        .ok_or_else(|| AtlasError::Source("invalid event URL path".to_owned()))?
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    if item["EventItemEventId"].as_u64() != Some(event_id)
        || event_parts.len() != 4
        || event_parts[0] != "v1"
        || event_parts[2] != "events"
        || event_parts[3] != event_id.to_string()
    {
        return Err(AtlasError::Source(
            "decision event ID differs from official event URL".to_owned(),
        ));
    }
    if decision.event_item_id.value.parse::<u64>().is_err()
        || decision.official_matter.value.trim().is_empty()
    {
        return Err(AtlasError::Manifest(
            "invalid official matter identity".to_owned(),
        ));
    }
    let date = decision.date.value.as_bytes();
    if date.len() < 10
        || date[4] != b'-'
        || date[7] != b'-'
        || !date[..4]
            .iter()
            .chain(date[5..7].iter())
            .chain(date[8..10].iter())
            .all(u8::is_ascii_digit)
    {
        return Err(AtlasError::Manifest(
            "decision date lacks YYYY-MM-DD".to_owned(),
        ));
    }
    let raw_outcome = decision.outcome.value.to_ascii_lowercase();
    let expected = match raw_outcome.as_str() {
        "pass" | "passed" | "approved" | "adopted" => "approved",
        "fail" | "failed" | "rejected" | "denied" => "failed",
        "deferred" | "postponed" | "tabled" => "deferred",
        _ => "unknown",
    };
    if decision.outcome_class != expected {
        return Err(AtlasError::Manifest(
            "outcome class differs from official outcome".to_owned(),
        ));
    }
    if let Some(consent) = &manifest.consent_context {
        if manifest.review_state != "pending" {
            return Err(AtlasError::Manifest(
                "consent-block cases are preview-only until their release proof is completed"
                    .to_owned(),
            ));
        }
        if consent.item_consent.proof.source_id != *event_source_id
            || consent.item_consent.proof.locator != format!("{item_prefix}.EventItemConsent")
            || consent.item_consent.value != "1"
            || item["EventItemConsent"].as_u64() != Some(1)
            || !manifest
                .supporting_sources
                .contains(&consent.block_vote_source_id)
        {
            return Err(AtlasError::Source(
                "decision is not bound to a consent agenda item".to_owned(),
            ));
        }
        let block_prefix = consent
            .block_item_id
            .proof
            .locator
            .strip_suffix(".EventItemId")
            .ok_or_else(|| AtlasError::Source("consent motion lacks item locator".to_owned()))?;
        if !block_prefix.starts_with("$.EventItems[") || !block_prefix.ends_with(']') {
            return Err(AtlasError::Source(
                "consent motion is not an event item".to_owned(),
            ));
        }
        for (fact, field) in [
            (&consent.block_item_id, "EventItemId"),
            (&consent.block_title, "EventItemTitle"),
            (&consent.block_action, "EventItemActionText"),
            (&consent.block_outcome, "EventItemPassedFlagName"),
        ] {
            if fact.proof.source_id != *event_source_id
                || fact.proof.locator != format!("{block_prefix}.{field}")
            {
                return Err(AtlasError::Source(
                    "consent motion fields cross event items".to_owned(),
                ));
            }
        }
        let block = locate(&event, block_prefix)
            .ok_or_else(|| AtlasError::Source("consent motion disappeared".to_owned()))?;
        let arapahoe_consent = event_parts[1] == "arapahoe"
            && consent
                .block_title
                .value
                .eq_ignore_ascii_case("Approval of the Consent Agenda")
            && consent
                .block_action
                .value
                .to_ascii_lowercase()
                .contains("approve items on the consent agenda")
            && block["EventItemActionName"].as_str().is_some_and(|name| {
                name.to_ascii_lowercase()
                    .starts_with("approved the consent agenda")
            });
        let denver_consent = event_parts[1] == "denver"
            && consent.block_title.value.eq_ignore_ascii_case("Block Vote")
            && consent
                .block_action
                .value
                .to_ascii_lowercase()
                .contains("approve the following items en bloc")
            && block["EventItemActionName"] == "approved the Consent Agenda"
            && item["EventItemActionText"]
                .as_str()
                .is_some_and(|action| action.to_ascii_lowercase().contains("adopted en bloc"));
        if block["EventItemEventId"].as_u64() != Some(event_id)
            || consent.block_item_id.value == decision.event_item_id.value
            || consent.block_item_id.value.parse::<u64>().is_err()
            || !(arapahoe_consent || denver_consent)
            || consent.block_outcome.value != "Pass"
            || decision.outcome_class != "approved"
        {
            return Err(AtlasError::Source(
                "consent motion does not establish this approval".to_owned(),
            ));
        }
        let vote_source = &sources[&consent.block_vote_source_id];
        let vote_url = Url::parse(&vote_source.registered.url)
            .map_err(|_| AtlasError::Source("invalid consent vote URL".to_owned()))?;
        let vote_parts = vote_url
            .path_segments()
            .ok_or_else(|| AtlasError::Source("invalid consent vote URL path".to_owned()))?
            .map(str::to_ascii_lowercase)
            .collect::<Vec<_>>();
        let vote_rows: Value = serde_json::from_slice(&vote_source.bytes)?;
        let vote_rows = vote_rows
            .as_array()
            .ok_or_else(|| AtlasError::Source("consent votes are not an array".to_owned()))?;
        let mut vote_ids = BTreeSet::new();
        if vote_source.registered.format != "json"
            || vote_parts.len() != 5
            || vote_parts[0] != "v1"
            || vote_parts[1] != event_parts[1]
            || vote_parts[2] != "eventitems"
            || vote_parts[3] != consent.block_item_id.value
            || vote_parts[4] != "votes"
            || vote_rows.is_empty()
            || vote_rows.iter().any(|row| {
                row["VoteEventItemId"].as_u64().map(|id| id.to_string())
                    != Some(consent.block_item_id.value.clone())
                    || !row["VoteId"].as_u64().is_some_and(|id| vote_ids.insert(id))
            })
        {
            return Err(AtlasError::Source(
                "consent vote rows are not bound to the block motion".to_owned(),
            ));
        }
    }
    let mut vote_ids = BTreeSet::new();
    let mut person_ids = BTreeSet::new();
    for vote in &manifest.votes {
        let fields = [
            (&vote.person_name, "VotePersonName"),
            (&vote.recorded_vote, "VoteValueName"),
            (&vote.official_vote_id, "VoteId"),
            (&vote.official_person_id, "VotePersonId"),
            (&vote.decision_ref, "VoteEventItemId"),
        ];
        let source_id = &vote.person_name.proof.source_id;
        let mut base = None;
        for (field, expected_name) in fields {
            if &field.proof.source_id != source_id {
                return Err(AtlasError::Source(
                    "vote fields cross source records".to_owned(),
                ));
            }
            let row = field.proof.locator.rsplit_once('.').map(|(row, _)| row);
            if !row.is_some_and(|row| row.starts_with("$[") && row.ends_with(']'))
                || !field.proof.locator.ends_with(&format!(".{expected_name}"))
            {
                return Err(AtlasError::Source("vote locator is not a row".to_owned()));
            }
            if let Some(previous) = base {
                if row != Some(previous) {
                    return Err(AtlasError::Source("vote fields cross JSON rows".to_owned()));
                }
            } else {
                base = row;
            }
        }
        let vote_source = &sources[source_id];
        let vote_url = Url::parse(&vote_source.registered.url)
            .map_err(|_| AtlasError::Source("invalid vote source URL".to_owned()))?;
        let vote_parts = vote_url
            .path_segments()
            .ok_or_else(|| AtlasError::Source("invalid vote URL path".to_owned()))?
            .map(str::to_ascii_lowercase)
            .collect::<Vec<_>>();
        if vote_source.registered.format != "json"
            || vote_parts.len() != 5
            || vote_parts[0] != "v1"
            || vote_parts[1] != event_parts[1]
            || vote_parts[2] != "eventitems"
            || vote_parts[3] != decision.event_item_id.value
            || vote_parts[4] != "votes"
        {
            return Err(AtlasError::Source(
                "vote source URL is not the decision roll call".to_owned(),
            ));
        }
        if vote.decision_ref.value != decision.event_item_id.value
            || vote.official_vote_id.value.parse::<u64>().is_err()
            || vote.official_person_id.value.parse::<u64>().is_err()
            || !vote_ids.insert(&vote.official_vote_id.value)
            || !person_ids.insert(&vote.official_person_id.value)
        {
            return Err(AtlasError::Source(
                "vote identity or decision reference differs".to_owned(),
            ));
        }
    }
    if let Some(first) = manifest.votes.first() {
        let source_id = &first.person_name.proof.source_id;
        if manifest
            .votes
            .iter()
            .any(|vote| vote.person_name.proof.source_id != *source_id)
        {
            return Err(AtlasError::Source(
                "one roll call spans multiple source arrays".to_owned(),
            ));
        }
        let rows: Value = serde_json::from_slice(&sources[source_id].bytes)?;
        let rows = rows
            .as_array()
            .ok_or_else(|| AtlasError::Source("vote source is not a JSON array".to_owned()))?;
        let published_ids = manifest
            .votes
            .iter()
            .map(|vote| vote.official_vote_id.value.as_str())
            .collect::<BTreeSet<_>>();
        let raw_ids = rows
            .iter()
            .map(|row| row["VoteId"].as_u64().map(|id| id.to_string()))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| AtlasError::Source("vote row lacks ID".to_owned()))?;
        let raw_ids = raw_ids.iter().map(String::as_str).collect::<BTreeSet<_>>();
        if rows.len() != manifest.votes.len()
            || raw_ids != published_ids
            || rows.iter().any(|row| {
                row["VoteEventItemId"].as_u64().map(|id| id.to_string())
                    != Some(decision.event_item_id.value.clone())
            })
        {
            return Err(AtlasError::Source(
                "published votes omit or misattribute official roll-call rows".to_owned(),
            ));
        }
    }
    Ok(())
}

pub(super) fn requirement(checked: &Checked) -> AtlasReviewRequirement {
    AtlasReviewRequirement {
        id: stable_id("atlas-release", &[&checked.manifest.case_id]),
        bound_digest: checked.binding_digest.clone(),
        kind: if checked.manifest.review_state == "source_verified" {
            "auto_source_verified"
        } else {
            "release"
        }
        .to_owned(),
        summary: format!("Release atlas case {}", checked.manifest.case_id),
        source_url: String::new(),
        locator: String::new(),
        source_sha256: checked
            .binding_digest
            .trim_start_matches("sha256:")
            .to_owned(),
    }
}

#[allow(clippy::too_many_lines)]
fn verify_acquisition_receipts(store: &Store, checked: &Checked) -> Result<(), AtlasError> {
    for source in checked.sources.values() {
        let item = &source.registered;
        let acquisition_id = item.acquisition_source_id.as_deref().ok_or_else(|| {
            AtlasError::Review(format!(
                "source lacks reviewed archive configuration binding: {}",
                item.id
            ))
        })?;
        let expected_config = item.acquisition_config_digest.as_deref().ok_or_else(|| {
            AtlasError::Review(format!(
                "source lacks reviewed archive config digest: {}",
                item.id
            ))
        })?;
        let observed = store.fetch_observations(acquisition_id)?;
        let review = store
            .current_source_review(acquisition_id)?
            .ok_or_else(|| {
                AtlasError::Review(format!("source lacks current policy review: {}", item.id))
            })?;
        let source_url = Url::parse(&item.url)
            .map_err(|_| AtlasError::Review("invalid reviewed source URL".to_owned()))?;
        let host = source_url.host_str().unwrap_or_default();
        let robots_url = format!("https://{host}/robots.txt");
        let reviewed_at = OffsetDateTime::parse(&review.reviewed_at, &Rfc3339)
            .map_err(|_| AtlasError::Review("source review date is invalid".to_owned()))?;
        // The source-review CLI accepts either an RFC 3339 instant or a UTC
        // date, and treats the latter as valid through that date. Apply the
        // same expiry interpretation as live source archival.
        let expiry = if review.expires_at.len() == 10 {
            format!("{}T23:59:59Z", review.expires_at)
        } else {
            review.expires_at.clone()
        };
        let expires_at = OffsetDateTime::parse(&expiry, &Rfc3339)
            .map_err(|_| AtlasError::Review("source review expiry is invalid".to_owned()))?;
        let now = OffsetDateTime::now_utc();
        if review.source_id != acquisition_id
            || expected_config.len() != 64
            || review.source_config_digest != expected_config
            || !human_reviewer(&review.reviewer)
            || !review
                .reviewed_hosts
                .iter()
                .any(|approved| approved == host)
            || !review
                .endpoint_patterns
                .iter()
                .any(|path| path == source_url.path())
            || review.robots_url != robots_url
            || review.robots_snapshot_digest.len() != 64
            || review.robots_provenance.is_none()
            || !review.restrictions.is_empty()
            || reviewed_at >= expires_at
            || now > expires_at
        {
            return Err(AtlasError::Review(format!(
                "source policy review is expired or out of scope: {}",
                item.id
            )));
        }
        let robots_digest = &review.robots_snapshot_digest;
        let robots_blob = store.content_path(robots_digest)?;
        let mut robots_bytes = Vec::new();
        fs::File::open(robots_blob)
            .map_err(|_| {
                AtlasError::Review(format!("source robots capture is unavailable: {}", item.id))
            })?
            .take(1_048_577)
            .read_to_end(&mut robots_bytes)?;
        if robots_bytes.len() > 1_048_576
            || sha256_hex(&robots_bytes) != *robots_digest
            || !observed.iter().any(|observation| {
                observation.id == review.robots_provenance.as_deref().unwrap_or_default()
                    && observation.requested_url == robots_url
                    && observation.body_digest.as_deref() == Some(robots_digest)
                    && observation.error.is_none()
            })
        {
            return Err(AtlasError::Review(format!(
                "source robots review lacks preserved provenance: {}",
                item.id
            )));
        }
        let mut receipts = observed
            .iter()
            .filter(|observation| {
                observation.source_id.as_deref() == Some(acquisition_id)
                    && observation.requested_url == item.url
                    && observation.final_url == item.url
                    && observation.method == "GET"
                    && matches!(observation.status_code, 200 | 304)
                    && observation.redirect_target.is_none()
                    && observation.error.is_none()
                    && OffsetDateTime::parse(&observation.retrieved_at, &Rfc3339)
                        .is_ok_and(|at| at <= now)
                    && !observation.resolved_ips.is_empty()
                    && observation
                        .resolved_ips
                        .iter()
                        .all(|address| address.parse::<IpAddr>().is_ok_and(is_public_address))
            })
            .collect::<Vec<_>>();
        receipts.sort_by(|left, right| left.retrieved_at.cmp(&right.retrieved_at));
        let mut current_digest = None;
        let mut observed_during_review = false;
        for receipt in receipts {
            let captured_at = OffsetDateTime::parse(&receipt.retrieved_at, &Rfc3339)
                .map_err(|_| AtlasError::Review("invalid source receipt date".to_owned()))?;
            if captured_at >= reviewed_at && captured_at < expires_at && captured_at <= now {
                observed_during_review = true;
            }
            if receipt.status_code == 200 {
                current_digest = receipt.body_digest.as_deref();
            } else if current_digest.is_none() {
                // A 304 with no earlier complete 200 cannot authenticate the
                // archived bytes on its own.
                return Err(AtlasError::Review(format!(
                    "source has unbound 304 receipt: {}",
                    item.id
                )));
            }
        }
        if !observed_during_review || current_digest != Some(item.sha256.as_str()) {
            return Err(AtlasError::Review(format!(
                "source lacks current matching reviewed archive receipt: {}",
                item.id
            )));
        }
        let blob_path = store.content_path(&item.sha256)?;
        let mut bytes = Vec::new();
        fs::File::open(&blob_path)
            .map_err(|_| AtlasError::Review(format!("source blob unavailable: {}", item.id)))?
            .take(MAX_FIXTURE_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_FIXTURE_BYTES
            || sha256_hex(&bytes) != item.sha256
            || bytes != source.bytes
        {
            return Err(AtlasError::Review(format!(
                "source blob differs from reviewed fixture: {}",
                item.id
            )));
        }
    }
    Ok(())
}

pub(super) fn production_gate(
    store: &Store,
    checked: &Checked,
    site_url: Option<&str>,
) -> Result<(), AtlasError> {
    if checked.manifest.synthetic || checked.manifest.review_state == "pending" {
        return Err(AtlasError::Review(
            "case is synthetic or pending source verification".to_owned(),
        ));
    }
    if !publication_allowlist_allows(store, ATLAS_RELEASE_CATEGORY)
        .map_err(|error| AtlasError::Review(error.to_string()))?
    {
        return Err(AtlasError::Review(
            "atlas_release allowlist is absent".to_owned(),
        ));
    }
    verify_acquisition_receipts(store, checked)?;
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
    let req = requirement(checked);
    let current = store.current_review(&req.id)?;
    if checked.manifest.review_state == "approved" {
        if !current.as_ref().is_some_and(|decision| {
            decision.state == ReviewState::Approved
                && decision.bound_digest == req.bound_digest
                && human_reviewer(&decision.reviewer)
        }) {
            return Err(AtlasError::Review(format!(
                "case lacks current human release approval: {}",
                req.id
            )));
        }
    } else if current.is_some_and(|decision| {
        decision.state != ReviewState::Approved
            || decision.bound_digest != req.bound_digest
            || !human_reviewer(&decision.reviewer)
    }) {
        return Err(AtlasError::Review(format!(
            "current case review vetoes automatic release: {}",
            req.id
        )));
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
pub(super) fn snapshot(
    store: &Store,
    checked: &Checked,
    mode: AtlasExportMode,
    site_url: Option<&str>,
) -> Result<Value, AtlasError> {
    let manifest = &checked.manifest;
    let preview = mode == AtlasExportMode::Preview;
    let review_state = if preview {
        "PENDING_RELEASE"
    } else {
        "SOURCE_VERIFIED"
    };
    let slug = manifest.case_id.replace('-', "_");
    let decision_id = format!("decision_{slug}");
    let jurisdiction_id = format!(
        "jurisdiction_{}",
        manifest.jurisdiction.id.replace('-', "_")
    );
    let agency_id = manifest
        .agency
        .as_ref()
        .map(|agency| format!("agency_{}", agency.id.replace('-', "_")));
    let mut evidence = Vec::new();
    let mut evidence_ids = BTreeMap::new();
    for (key, fact) in &checked.facts {
        let source = &checked.sources[&fact.proof.source_id].registered;
        let id = format!("evidence_{slug}_{key}");
        evidence.push(json!({
            "id":id, "source_id":source.id, "claim":fact.value,
            "quote":fact.proof.quote, "locator":fact.proof.locator,
            "authority":source.authority, "retrieved_at":null,
            "sha256":source.sha256, "review_state":review_state,
            "verification_digest":review_digest("atlas-v2-source-verification", &[
                &manifest.case_id,key,&source.url,&source.sha256,&fact.proof.locator,
                &fact.proof.quote,&fact.value
            ]), "corrections":[],
        }));
        evidence_ids.insert(key.as_str(), id);
    }
    let evidence_id = |key: &str| evidence_ids.get(key).expect("checked fact");
    let fact_item = |key: &str, label: &str, value: &str| {
        json!({
            "label":label,"value":value,"evidence_id":evidence_id(key),
            "review_state":review_state,"corrections":[]
        })
    };
    // The atlas joins this verified jurisdiction ID to an archived geography
    // roster. Case manifests cannot place arbitrary points on the map.
    let geo = Value::Null;
    let date = &manifest.decision.date.value[..10];
    let decision_name = manifest.decision.display_title.clone().unwrap_or_else(|| {
        manifest.decision.short_label.as_ref().map_or_else(
            || manifest.decision.title.value.clone(),
            |short| {
                format!(
                    "{}: {}",
                    manifest.decision.official_matter.value, short.value
                )
            },
        )
    });
    let mut entities = Vec::new();
    let place_name = manifest
        .jurisdiction
        .display_name
        .as_deref()
        .unwrap_or(&manifest.jurisdiction.name.value);
    entities.push(json!({
        "id":jurisdiction_id,"type":"JURISDICTION","name":place_name,
        "jurisdiction_ref":manifest.jurisdiction.jurisdiction_ref,
        "subtitle":"Colorado jurisdiction","status":review_state,
        "description":"Jurisdiction named in an official decision record",
        "geo":geo,"jurisdiction_ids":[],"aliases":[],"tags":["Colorado"],
        "updated_at":date,"source_count":1,"relation_count":0,
        "attributes":[fact_item("jurisdiction","Official jurisdiction name",&manifest.jurisdiction.name.value)],
        "relationships":[],"timeline":[],
        "source_ids":[manifest.jurisdiction.name.proof.source_id],
    }));
    if let Some(agency) = &manifest.agency {
        entities.push(json!({
            "id":agency_id,"type":"AGENCY","name":agency.name.value,
            "subtitle":"Agency named in official record","status":review_state,
            "description":"Agency named in the cited decision record",
            "geo":geo,"jurisdiction_ids":[jurisdiction_id],"aliases":[],"tags":[],
            "updated_at":date,"source_count":1,"relation_count":0,
            "attributes":[fact_item("agency","Official agency name",&agency.name.value)],
            "relationships":[],"timeline":[],"source_ids":[agency.name.proof.source_id],
        }));
    }
    let mut relationships = vec![super::relationship(
        &format!("{slug}_jurisdiction"),
        "WITHIN_JURISDICTION",
        "Jurisdiction named in official record",
        &jurisdiction_id,
        &(
            evidence_id("jurisdiction").clone(),
            review_state.to_owned(),
            json!([]),
        ),
    )];
    if let Some(agency) = &manifest.agency {
        relationships.push(super::relationship(
            &format!("{slug}_agency"),
            "NAMES_AGENCY",
            "Agency named in official record",
            agency_id.as_deref().expect("agency id"),
            &(
                evidence_id("agency").clone(),
                review_state.to_owned(),
                json!([]),
            ),
        ));
        let _ = agency;
    }
    if let Some(agreement) = &manifest.agreement_context {
        let agreement_id = format!("agreement_{slug}");
        let agreement_name = agreement
            .display_name
            .as_deref()
            .unwrap_or(&agreement.name.value);
        let (relation_type, relation_label, agreement_subtitle) =
            if agreement.status == "award_authorized" {
                (
                    "AUTHORIZED_AGREEMENT",
                    "Public body authorized an agreement; execution not established",
                    "Authorized agreement; execution not established",
                )
            } else if agreement.status == "proposal_approved" {
                (
                    "APPROVED_PROPOSED_AGREEMENT",
                    "Public body approved a proposed agreement; execution not established",
                    "Approved proposal; execution not established",
                )
            } else {
                (
                    "APPROVED_AGREEMENT_FORMAT",
                    "Public body approved agreement format; execution not established",
                    "Approved agreement format; execution not established",
                )
            };
        relationships.push(super::relationship(
            &format!("{slug}_agreement"),
            relation_type,
            relation_label,
            &agreement_id,
            &(
                evidence_id("agreement").clone(),
                review_state.to_owned(),
                json!([]),
            ),
        ));
        let mut agreement_attributes = vec![
            fact_item(
                "agreement",
                "Agreement named in decision",
                &agreement.name.value,
            ),
            fact_item(
                "decision_outcome",
                "Recorded authorization outcome",
                &manifest.decision.outcome.value,
            ),
        ];
        if let Some(total) = &agreement.proposed_total {
            agreement_attributes.push(fact_item(
                "agreement_proposed_total",
                "Proposed agreement total, not payment",
                &total.value,
            ));
        }
        if let Some(increase) = &agreement.proposed_increase {
            agreement_attributes.push(fact_item(
                "agreement_proposed_increase",
                "Proposed increase, not payment",
                &increase.value,
            ));
        }
        entities.push(json!({
            "id":agreement_id,"type":"AGREEMENT","name":agreement_name,
            "subtitle":agreement_subtitle,
            "status":review_state,
            "description":"The official decision title records agreement authorization; the cited records do not establish signed execution or payments.",
            "jurisdiction_ids":[jurisdiction_id],"aliases":[],"tags":["agreement context"],
            "updated_at":date,"source_count":1,"relation_count":0,
            "attributes":agreement_attributes,
            "relationships":[],"timeline":[],"source_ids":[agreement.name.proof.source_id],
            "unknowns":[{"kind":"contract_execution","state":"not_established"},
                {"kind":"payment","state":"not_established"}],
        }));
    }
    let mut votes = Vec::new();
    for (index, vote) in manifest.votes.iter().enumerate() {
        // A person name alone is not an identity key. The cited roll call
        // supplies a Legistar person ID, scoped to this portal client and
        // the verified jurisdiction. Distinct clients never share an entity.
        let vote_source = &checked.sources[&vote.person_name.proof.source_id].registered;
        let vote_url = Url::parse(&vote_source.url)
            .map_err(|_| AtlasError::Source("invalid verified vote URL".to_owned()))?;
        let client = vote_url
            .path_segments()
            .and_then(|mut segments| segments.nth(1).map(str::to_owned))
            .ok_or_else(|| AtlasError::Source("verified vote lacks portal client".to_owned()))?;
        if !client
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err(AtlasError::Source(
                "verified vote has unsafe portal client".to_owned(),
            ));
        }
        let voter_id = format!(
            "recorded_voter_{}_{}_{}",
            manifest.jurisdiction.id.replace('-', "_"),
            client.replace('-', "_"),
            vote.official_person_id.value
        );
        let key = format!("vote_{index}_vote");
        let name_key = format!("vote_{index}_name");
        let person_key = format!("vote_{index}_person_id");
        relationships.push(super::relationship(
            &format!("{slug}_vote_{}", vote.official_vote_id.value),
            "RECORDED_VOTE",
            &format!("Recorded {} vote", vote.recorded_vote.value),
            &voter_id,
            &(
                evidence_id(&key).clone(),
                review_state.to_owned(),
                json!([]),
            ),
        ));
        votes.push(json!({
            "name":vote.person_name.value,"vote":vote.recorded_vote.value,
            "voter_entity_id":voter_id,"office_role":"not_established",
            "office_term":"not_established","evidence_id":evidence_id(&key),
            "review_state":review_state,"corrections":[],
        }));
        entities.push(json!({
            "id":voter_id,"type":"RECORDED_VOTER","name":vote.person_name.value,
            "jurisdiction_ref":manifest.jurisdiction.jurisdiction_ref,
            "identity_scope":format!("legistar:{client}"),
            "official_person_id":vote.official_person_id.value,
            "subtitle":"Recorded voter; office role and term not established",
            "status":review_state,
            "description":"The cited roll calls use the same official portal person ID and exact name in this jurisdiction. Public office role and term are not established by these sources.",
            "jurisdiction_ids":[jurisdiction_id],"aliases":[],"tags":["public roll call"],
            "updated_at":date,"source_count":1,"relation_count":0,
            "attributes":[fact_item(&name_key,"Name in roll call",&vote.person_name.value),
                fact_item(&person_key,"Official portal person ID",&vote.official_person_id.value),
                fact_item(&key,&format!("Vote on {decision_name}"),&vote.recorded_vote.value)],
            "relationships":[],
            "timeline":[{"id":format!("{slug}_vote_{}",vote.official_vote_id.value),
                "date":date,"type":"RECORDED_VOTE",
                "title":format!("{} on {decision_name}",vote.recorded_vote.value),
                "evidence_id":evidence_id(&key),"review_state":review_state,"corrections":[]}],
            "source_ids":[vote.person_name.proof.source_id],
            "unknowns":[{"kind":"office_role","state":"not_established"},{"kind":"office_term","state":"not_established"}],
        }));
    }
    let mut technologies = Vec::new();
    let technology_relationship = if manifest.agreement_context.is_some() {
        "Named in an authorized agreement decision; execution and payments are not established"
    } else if manifest.consent_context.is_some() {
        "Named in an approved consent item; signed execution and payments are not established"
    } else if manifest.decision.outcome_class == "failed" {
        "Named in a proposal that failed; this vote did not authorize it"
    } else {
        "Named in the official decision record; a specific contract link is not established"
    };
    for (index, context) in manifest.technology_context.iter().enumerate() {
        let key = format!("technology_{index}");
        technologies.push(json!({
            "name":context.name.value,"context":technology_relationship,
            "evidence_id":evidence_id(&key),"review_state":review_state,"corrections":[],
        }));
    }
    let mut source_ids = checked.sources.keys().cloned().collect::<Vec<_>>();
    source_ids.sort();
    let share_url = if preview {
        None
    } else {
        site_url.map(|base| {
            format!(
                "{}/decisions/{}.html",
                base.trim_end_matches('/'),
                manifest.case_id
            )
        })
    };
    let mut attributes = vec![
        fact_item(
            "decision_outcome",
            "Recorded outcome",
            &manifest.decision.outcome.value,
        ),
        fact_item(
            "decision_matter",
            "Official matter",
            &manifest.decision.official_matter.value,
        ),
    ];
    if let Some(action) = &manifest.decision.action {
        attributes.push(fact_item(
            "decision_action",
            "Recorded action",
            &action.value,
        ));
    }
    if manifest.consent_context.is_some() {
        attributes.push(fact_item(
            "consent_item_consent",
            "Consent agenda item (1 = yes)",
            "1",
        ));
    }
    let timeline_title = format!("{decision_name}: {}", manifest.decision.outcome.value);
    let mut tally = BTreeMap::<&str, usize>::new();
    for vote in &manifest.votes {
        *tally.entry(vote.recorded_vote.value.as_str()).or_default() += 1;
    }
    let mut ordered_votes = Vec::new();
    for label in ["Aye", "Yes", "Nay", "No", "Abstain", "Abstained", "Absent"] {
        if let Some(count) = tally.remove(label) {
            ordered_votes.push(format!("{count} {label}"));
        }
    }
    ordered_votes.extend(
        tally
            .into_iter()
            .map(|(label, count)| format!("{count} {label}")),
    );
    let ordered_tally = ordered_votes.join(", ");
    let outcome_text = match manifest.decision.outcome_class.as_str() {
        "approved" => "approved",
        "failed" => "did not approve",
        "deferred" => "deferred",
        _ => "recorded an unresolved outcome for",
    };
    let summary = if manifest.consent_context.is_some() {
        format!(
            "{place_name} {outcome_text} {decision_name} on {date} as a consent agenda item. The recorded roll call belongs to the consent block; individual votes on this item are not established. {}",
            manifest.contract_link.detail
        )
    } else {
        format!(
            "{place_name} {outcome_text} {decision_name} on {date}. The direct roll call records {ordered_tally}. {}",
            manifest.contract_link.detail
        )
    };
    let mut unknowns = vec![json!({
        "kind":if manifest.agreement_context.is_some() || manifest.consent_context.is_some() {"contract_execution"} else {"contract_award"},
        "state":"not_established","detail":manifest.contract_link.detail
    })];
    let consent_record = manifest.consent_context.as_ref().map(|consent| {
        unknowns.push(json!({"kind":"individual_item_votes","state":"not_established",
            "detail":"The published roll call belongs to the consent agenda motion, not this item."}));
        json!({
            "motion_item_id":consent.block_item_id.value,
            "motion_title":consent.block_title.value,
            "motion_outcome":consent.block_outcome.value,
            "block_vote_source_id":consent.block_vote_source_id,
            "motion_evidence_id":evidence_id("consent_block_item_id"),
            "motion_title_evidence_id":evidence_id("consent_block_title"),
            "motion_action_evidence_id":evidence_id("consent_block_action"),
            "motion_outcome_evidence_id":evidence_id("consent_block_outcome"),
            "item_consent_evidence_id":evidence_id("consent_item_consent"),
            "note":"Block roll call covers multiple items; no item-specific voter attribution is published."
        })
    });
    entities.push(json!({
        "id":decision_id,"slug":manifest.case_id,"type":"DECISION",
        "name":decision_name,
        "subtitle":format!("{} • {}",date,manifest.decision.outcome.value),
        "status":review_state,
        "description":summary,"summary":summary,
        "geo":geo,"jurisdiction_ids":[jurisdiction_id],
        "aliases":[manifest.decision.official_matter.value],
        "tags":["official decision",manifest.decision.outcome_class],
        "updated_at":date,"source_count":source_ids.len(),"relation_count":relationships.len(),
        "attributes":attributes,"relationships":relationships,
        "timeline":[{"id":format!("{slug}_decision"),"date":date,"type":"RECORDED_DECISION",
            "title":timeline_title,"evidence_id":evidence_id("decision_outcome"),
            "review_state":review_state,"corrections":[]}],
        "source_ids":source_ids,"votes":votes,"vote_record_state":manifest.vote_record_state,
        "consent_block":consent_record,"technology_context":technologies,
        "unknowns":unknowns,
        "unresolved_question":manifest.unresolved_question,"share_url":share_url,
    }));
    entities.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    let req = requirement(checked);
    let history = store.reviews_for_citation(&req.id)?;
    let corrections = history
        .iter()
        .filter(|review| review.state != ReviewState::Approved)
        .map(|review| json!({"state":review.state.label(),"at":review.decided_at}))
        .collect::<Vec<_>>();
    let release_review = if preview {
        json!({"state":"PENDING_RELEASE","bound_digest":req.bound_digest,
            "reviewed_at":null,"corrections":corrections})
    } else if manifest.review_state == "source_verified" {
        json!({"state":"SOURCE_VERIFIED","bound_digest":req.bound_digest,
            "reviewed_at":null,"verification_method":"preserved_source_bytes_and_exact_locators",
            "corrections":corrections})
    } else {
        let current = history.last().expect("production gate checked release");
        json!({"state":"APPROVED","bound_digest":req.bound_digest,
            "reviewed_at":current.decided_at,"corrections":corrections})
    };
    let claim_sources = checked
        .facts
        .values()
        .map(|fact| fact.proof.source_id.as_str())
        .collect::<BTreeSet<_>>();
    let sources = checked.sources.values().map(|source| {
        let item = &source.registered;
        json!({
            "id":item.id,"title":item.title,"authority":item.authority,
            "source_type":"official_record","canonical_url":item.url,
            "retrieved_at":null,"document_date":null,"sha256":item.sha256,
            "description":if manifest.consent_context.as_ref().is_some_and(|consent| consent.block_vote_source_id == item.id) {
                "Preserved consent-block roll call; every row checked against the block motion"
            } else if !claim_sources.contains(item.id.as_str()) {
                "Preserved official record; SHA-256 verified, not used for machine-verified claims"
            } else {
                "Preserved official record; exact digest and locator verified against local raw bytes"
            }
        })
    }).collect::<Vec<_>>();
    let relationship_count = entities
        .iter()
        .map(|entity| entity["relationships"].as_array().map_or(0, Vec::len))
        .sum::<usize>();
    Ok(json!({
        "schema_version":"pnull-public-snapshot-v1","demo":manifest.synthetic,"preview":preview,
        "status":{"state":if preview {"pending_release"} else {"released"},
            "dataset_version":checked.binding_digest,"last_ingest":null,"last_publish":null,
            "manifest_digest":checked.binding_digest,
            "counts":{"entities":entities.len(),"records":evidence.len(),"sources":sources.len(),
                "relationships":relationship_count}},
        "release_review":release_review,"activity":[],"entities":entities,
        "evidence":evidence,"sources":sources,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pnull_core::{FetchObservation, PublicationAllowlist, ReviewDecision, SourceReview};
    use std::path::PathBuf;

    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn manifest(name: &str) -> PathBuf {
        repo().join("fixtures/atlas/cases").join(name)
    }

    fn find_sources(value: &Value, ids: &mut BTreeSet<String>) {
        match value {
            Value::Object(fields) => {
                if let Some(id) = fields.get("source_id").and_then(Value::as_str) {
                    ids.insert(id.to_owned());
                }
                for nested in fields.values() {
                    find_sources(nested, ids);
                }
            }
            Value::Array(items) => {
                for item in items {
                    find_sources(item, ids);
                }
            }
            _ => {}
        }
    }

    fn copy_case(name: &str) -> (tempfile::TempDir, PathBuf) {
        copy_case_from(&format!("cases/{name}"))
    }

    fn copy_case_from(relative: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let case_path = root.join("fixtures/atlas").join(relative);
        fs::create_dir_all(case_path.parent().expect("case parent")).expect("case directory");
        fs::copy(repo().join("fixtures/atlas").join(relative), &case_path).expect("case fixture");
        let registry_path = root.join(REGISTRY_PATH);
        fs::copy(repo().join(REGISTRY_PATH), &registry_path).expect("registry fixture");
        let geography_path = root.join(GEOGRAPHY_PATH);
        fs::create_dir_all(geography_path.parent().expect("geography parent"))
            .expect("geography directory");
        fs::copy(repo().join(GEOGRAPHY_PATH), geography_path).expect("geography fixture");
        let case: Value =
            serde_json::from_slice(&fs::read(&case_path).expect("case bytes")).expect("case JSON");
        let registry: Value =
            serde_json::from_slice(&fs::read(&registry_path).expect("registry bytes"))
                .expect("registry JSON");
        let mut needed = BTreeSet::new();
        find_sources(&case, &mut needed);
        for source in case["supporting_sources"]
            .as_array()
            .expect("supporting sources")
        {
            needed.insert(source.as_str().expect("source id").to_owned());
        }
        for source in registry["sources"].as_array().expect("registry sources") {
            let id = source["id"].as_str().expect("registry id");
            if !needed.contains(id) {
                continue;
            }
            let relative = source["fixture"].as_str().expect("registry fixture");
            let destination = root.join(relative);
            fs::create_dir_all(destination.parent().expect("source parent"))
                .expect("source directory");
            fs::copy(repo().join(relative), destination).expect("copy preserved source");
        }
        (dir, case_path)
    }

    fn edit_json(path: &Path, edit: impl FnOnce(&mut Value)) {
        let mut value: Value =
            serde_json::from_slice(&fs::read(path).expect("read JSON")).expect("parse JSON");
        edit(&mut value);
        fs::write(
            path,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&value).expect("encode")
            ),
        )
        .expect("write JSON");
    }

    fn allow_release(store: &Store) {
        store
            .insert_publication_allowlist(&PublicationAllowlist {
                id: "atlas-v2-test-release".to_owned(),
                field_categories: vec![ATLAS_RELEASE_CATEGORY.to_owned()],
                created_at: "2026-11-10T00:00:00Z".to_owned(),
                note: "source verified tests".to_owned(),
            })
            .expect("allowlist");
    }

    fn seed_receipts(store: &Store, cases: &[PathBuf]) {
        for case in cases {
            let checked = inspect(case).expect("verified case for receipt");
            for source in checked.sources.values() {
                let registered = &source.registered;
                let acquisition_id = registered
                    .acquisition_source_id
                    .as_deref()
                    .expect("test acquisition source binding");
                let parsed_url = Url::parse(&registered.url).expect("source URL");
                let host = parsed_url.host_str().expect("host");
                let robots_url = format!("https://{host}/robots.txt");
                let robots_bytes = b"User-agent: *\nAllow: /\n";
                let robots_digest = sha256_hex(robots_bytes);
                store
                    .persist_content(&robots_digest, robots_bytes)
                    .expect("robots blob");
                let robots_receipt = FetchObservation {
                    id: stable_id("atlas-test-robots", &[acquisition_id]),
                    source_id: Some(acquisition_id.to_owned()),
                    requested_url: robots_url.clone(),
                    resolved_ips: vec!["1.1.1.1".to_owned()],
                    retrieved_at: "2026-11-10T10:00:00Z".to_owned(),
                    method: "GET".to_owned(),
                    status_code: 200,
                    redirect_target: None,
                    final_url: robots_url.clone(),
                    allowlisted_headers: vec![],
                    content_type: None,
                    content_length: Some(robots_bytes.len() as u64),
                    etag: None,
                    last_modified: None,
                    body_digest: Some(robots_digest.clone()),
                    error: None,
                };
                store
                    .insert_fetch_observation(&robots_receipt)
                    .expect("robots receipt");
                store
                    .insert_source_review(&SourceReview {
                        id: SourceReview::id_for(acquisition_id, "2026-11-10T11:00:00Z"),
                        source_id: acquisition_id.to_owned(),
                        source_config_digest: registered
                            .acquisition_config_digest
                            .clone()
                            .expect("test config digest"),
                        reviewed_hosts: vec![host.to_owned()],
                        endpoint_patterns: vec![parsed_url.path().to_owned()],
                        robots_url,
                        robots_snapshot_digest: robots_digest,
                        robots_provenance: Some(robots_receipt.id),
                        terms_urls: vec![],
                        terms_snapshot_digests: vec![],
                        reviewer: "Ada Editor".to_owned(),
                        note: "test policy".to_owned(),
                        reviewed_at: "2026-11-10T11:00:00Z".to_owned(),
                        expires_at: "2099-01-01T00:00:00Z".to_owned(),
                        minimum_interval_seconds: 3600,
                        restrictions: vec![],
                        supersedes: None,
                    })
                    .expect("source review");
                store
                    .persist_content(&registered.sha256, &source.bytes)
                    .expect("preserve source blob");
                let receipt = FetchObservation {
                    id: FetchObservation::id_for(&registered.url, "2026-11-10T12:00:00Z", 200),
                    source_id: Some(acquisition_id.to_owned()),
                    requested_url: registered.url.clone(),
                    resolved_ips: vec!["1.1.1.1".to_owned()],
                    retrieved_at: "2026-11-10T12:00:00Z".to_owned(),
                    method: "GET".to_owned(),
                    status_code: 200,
                    redirect_target: None,
                    final_url: registered.url.clone(),
                    allowlisted_headers: vec![],
                    content_type: None,
                    content_length: Some(source.bytes.len() as u64),
                    etag: None,
                    last_modified: None,
                    body_digest: Some(registered.sha256.clone()),
                    error: None,
                };
                store
                    .insert_fetch_observation(&receipt)
                    .expect("source receipt");
            }
        }
    }

    #[test]
    fn commerce_source_verified_case_publishes_without_per_case_review() {
        let (dir, case) = copy_case("commerce-city-ord-2755-2026-04-06.v2.json");
        let store = Store::open(dir.path().join("db")).expect("store");
        let output = dir.path().join("public.json");
        let requirement = requirement(&inspect(&case).expect("checked case"));
        assert!(
            store
                .current_review(&requirement.id)
                .expect("review query")
                .is_none()
        );
        assert!(matches!(
            super::super::export_atlas(
                &store,
                &case,
                &output,
                AtlasExportMode::Production,
                Some("https://panopticon.fail")
            ),
            Err(AtlasError::Review(_))
        ));
        allow_release(&store);
        assert!(matches!(
            super::super::export_atlas(
                &store,
                &case,
                &output,
                AtlasExportMode::Production,
                Some("https://panopticon.fail")
            ),
            Err(AtlasError::Review(_))
        ));
        seed_receipts(&store, std::slice::from_ref(&case));
        super::super::export_atlas(
            &store,
            &case,
            &output,
            AtlasExportMode::Production,
            Some("https://panopticon.fail"),
        )
        .expect("automatic source-verified release");
        let published: Value =
            serde_json::from_slice(&fs::read(&output).expect("public bytes")).expect("public JSON");
        assert_eq!(published["release_review"]["state"], "SOURCE_VERIFIED");
        assert!(published["release_review"]["reviewed_at"].is_null());
        let decision = published["entities"]
            .as_array()
            .expect("entities")
            .iter()
            .find(|entity| entity["type"] == "DECISION")
            .expect("decision");
        assert_eq!(
            decision["share_url"],
            "https://panopticon.fail/decisions/co-commerce-city-ord-2755-2026-04-06.html"
        );
        assert_eq!(decision["votes"].as_array().expect("votes").len(), 9);
        assert_eq!(decision["unknowns"][0]["kind"], "contract_execution");
        assert!(
            decision["relationships"]
                .as_array()
                .expect("relationships")
                .iter()
                .any(|relation| relation["type"] == "APPROVED_AGREEMENT_FORMAT")
        );
        assert!(
            store
                .current_review(&requirement.id)
                .expect("review query")
                .is_none()
        );
    }

    #[test]
    fn arapahoe_consent_case_stays_preview_only_without_individual_vote_attribution() {
        let (dir, case) = copy_case_from("preview-cases/arapahoe-county-24-633-2024-11-26.v2.json");
        let store = Store::open(dir.path().join("db")).expect("store");
        let output = dir.path().join("consent-preview.json");
        super::super::export_atlas(&store, &case, &output, AtlasExportMode::Preview, None)
            .expect("preview consent item");
        let published: Value = serde_json::from_slice(&fs::read(&output).expect("preview bytes"))
            .expect("preview JSON");
        let decision = published["entities"]
            .as_array()
            .expect("entities")
            .iter()
            .find(|entity| entity["type"] == "DECISION")
            .expect("decision");
        assert_eq!(decision["vote_record_state"], "consent_block");
        assert!(decision["votes"].as_array().expect("votes").is_empty());
        assert_eq!(decision["consent_block"]["motion_item_id"], "38020");
        assert!(
            decision["summary"]
                .as_str()
                .expect("summary")
                .contains("individual votes on this item are not established")
        );
        assert!(matches!(
            super::super::export_atlas(
                &store,
                &case,
                &dir.path().join("public.json"),
                AtlasExportMode::Production,
                Some("https://panopticon.fail")
            ),
            Err(AtlasError::Review(_))
        ));

        edit_json(&case, |value| {
            value["review_state"] = json!("source_verified");
        });
        assert!(
            inspect(&case).is_err(),
            "consent item escaped preview-only gate"
        );
        edit_json(&case, |value| {
            value["review_state"] = json!("pending");
        });
        let vote_path = dir
            .path()
            .join("fixtures/co-arapahoe/votes-38020-consent-2024-11-26.json");
        let mut rows: Value =
            serde_json::from_slice(&fs::read(&vote_path).expect("vote bytes")).expect("votes");
        rows[0]["VoteEventItemId"] = json!(37871);
        let tampered = serde_json::to_vec(&rows).expect("tampered rows");
        fs::write(&vote_path, &tampered).expect("write tampered vote source");
        edit_json(&dir.path().join(REGISTRY_PATH), |registry| {
            let source = registry["sources"]
                .as_array_mut()
                .expect("sources")
                .iter_mut()
                .find(|item| item["id"] == "co-arapahoe-votes-38020-consent-2024-11-26")
                .expect("consent vote source");
            source["sha256"] = json!(sha256_hex(&tampered));
        });
        assert!(
            inspect(&case).is_err(),
            "item vote spoof escaped block binding"
        );
    }

    #[test]
    fn arapahoe_expansion_and_denver_axon_consent_items_keep_block_votes_separate() {
        for (relative, matter, block_id, agreement_edge) in [
            (
                "preview-cases/arapahoe-county-25-378-2025-07-22.v2.json",
                "25-378",
                "40833",
                false,
            ),
            (
                "preview-cases/denver-25-1967-2025-12-08.v2.json",
                "25-1967",
                "199484",
                true,
            ),
            (
                "preview-cases/denver-25-1968-2025-12-08.v2.json",
                "25-1968",
                "199484",
                true,
            ),
        ] {
            let (dir, case) = copy_case_from(relative);
            let checked = inspect(&case).expect("source-backed consent preview");
            assert_eq!(checked.manifest.review_state, "pending");
            assert_eq!(checked.manifest.decision.official_matter.value, matter);
            assert!(checked.manifest.votes.is_empty());
            let store = Store::open(dir.path().join("db")).expect("store");
            let output = dir.path().join("preview.json");
            super::super::export_atlas(&store, &case, &output, AtlasExportMode::Preview, None)
                .expect("consent preview export");
            let snapshot: Value =
                serde_json::from_slice(&fs::read(&output).expect("preview bytes"))
                    .expect("preview JSON");
            let decision = snapshot["entities"]
                .as_array()
                .expect("entities")
                .iter()
                .find(|entity| entity["type"] == "DECISION")
                .expect("decision");
            assert_eq!(decision["vote_record_state"], "consent_block");
            assert!(decision["votes"].as_array().expect("votes").is_empty());
            assert_eq!(decision["consent_block"]["motion_item_id"], block_id);
            assert!(
                decision["unknowns"]
                    .as_array()
                    .expect("unknowns")
                    .iter()
                    .any(|gap| {
                        gap["kind"] == "individual_item_votes" && gap["state"] == "not_established"
                    })
            );
            let has_agreement = decision["relationships"]
                .as_array()
                .expect("relationships")
                .iter()
                .any(|relation| relation["type"] == "APPROVED_PROPOSED_AGREEMENT");
            assert_eq!(has_agreement, agreement_edge);
            if agreement_edge {
                let agreement_id = decision["relationships"]
                    .as_array()
                    .expect("relationships")
                    .iter()
                    .find(|relation| relation["type"] == "APPROVED_PROPOSED_AGREEMENT")
                    .expect("proposal edge")["target_entity_id"]
                    .as_str()
                    .expect("agreement ID");
                let agreement = snapshot["entities"]
                    .as_array()
                    .expect("entities")
                    .iter()
                    .find(|entity| entity["id"] == agreement_id)
                    .expect("agreement entity");
                assert!(
                    agreement["attributes"]
                        .as_array()
                        .expect("attributes")
                        .iter()
                        .any(|attribute| attribute["label"]
                            == "Proposed agreement total, not payment")
                );
            }
            assert!(matches!(
                super::super::export_atlas(
                    &store,
                    &case,
                    &dir.path().join("public.json"),
                    AtlasExportMode::Production,
                    Some("https://panopticon.fail")
                ),
                Err(AtlasError::Review(_))
            ));
        }
    }

    #[test]
    fn denver_consent_motion_action_and_proposal_status_are_bound() {
        let (_amount_dir, amount_case) =
            copy_case_from("preview-cases/denver-25-1967-2025-12-08.v2.json");
        edit_json(&amount_case, |value| {
            value["agreement_context"]["proposed_total"]["value"] = json!("$450,000.00");
            value["agreement_context"]["proposed_total"]["proof"]["quote"] = json!("$450,000.00");
        });
        assert!(
            inspect(&amount_case).is_err(),
            "proposed increase was mislabeled as agreement total"
        );
        let (dir, case) = copy_case_from("preview-cases/denver-25-1968-2025-12-08.v2.json");
        edit_json(&case, |value| {
            value["agreement_context"]["status"] = json!("award_authorized");
        });
        assert!(
            inspect(&case).is_err(),
            "proposed agreement was promoted to award"
        );
        edit_json(&case, |value| {
            value["agreement_context"]["status"] = json!("proposal_approved");
        });
        let event_path = dir
            .path()
            .join("fixtures/co-denver/event-9898-2025-12-08.json");
        edit_json(&event_path, |event| {
            event["EventItems"][68]["EventItemActionName"] = json!("rejected the Consent Agenda");
        });
        let changed_bytes = fs::read(&event_path).expect("changed event bytes");
        edit_json(&dir.path().join(REGISTRY_PATH), |registry| {
            let source = registry["sources"]
                .as_array_mut()
                .expect("sources")
                .iter_mut()
                .find(|item| item["id"] == "co-denver-2025-axon-event")
                .expect("Axon event source");
            source["sha256"] = json!(sha256_hex(&changed_bytes));
        });
        assert!(
            inspect(&case).is_err(),
            "unapproved block motion was accepted"
        );
    }

    #[test]
    fn failed_denver_vote_and_castle_rock_meeting_extension_stay_distinct() {
        let denver = inspect(&manifest("denver-25-0413-2025-05-05.v2.json")).expect("Denver case");
        assert_eq!(denver.manifest.decision.outcome.value, "Fail");
        assert_eq!(denver.manifest.decision.outcome_class, "failed");
        assert_eq!(
            denver
                .manifest
                .votes
                .iter()
                .filter(|vote| vote.recorded_vote.value == "Nay")
                .count(),
            12
        );
        assert_eq!(
            denver
                .manifest
                .votes
                .iter()
                .filter(|vote| vote.recorded_vote.value == "Absent")
                .count(),
            1
        );
        assert!(denver.manifest.agreement_context.is_none());
        let castle =
            inspect(&manifest("castle-rock-res-2025-071.v2.json")).expect("Castle Rock case");
        assert_eq!(castle.manifest.decision.event_item_id.value, "66424");
        assert_ne!(castle.manifest.decision.event_item_id.value, "66558");
        assert_eq!(
            castle
                .manifest
                .votes
                .iter()
                .filter(|vote| vote.recorded_vote.value == "Abstain")
                .count(),
            1
        );
        assert_eq!(
            castle
                .manifest
                .votes
                .iter()
                .filter(|vote| vote.recorded_vote.value == "Yes")
                .count(),
            6
        );
        assert!(castle.manifest.agreement_context.is_some());
    }

    #[test]
    fn altered_outcome_item_vote_and_unestablished_link_are_rejected() {
        let (dir, case) = copy_case("commerce-city-ord-2755-2026-04-06.v2.json");
        let original = fs::read(&case).expect("original manifest");
        edit_json(&case, |value| {
            value["decision"]["outcome_class"] = json!("failed");
        });
        assert!(inspect(&case).is_err());
        fs::write(&case, &original).expect("reset case");
        edit_json(&case, |value| {
            value["decision"]["title"]["proof"]["locator"] =
                json!("$.EventItems[35].EventItemTitle");
            value["decision"]["title"]["proof"]["quote"] = json!("FLOCK EXPANSION PROJECT");
            value["decision"]["title"]["value"] = json!("FLOCK EXPANSION PROJECT");
        });
        assert!(matches!(inspect(&case), Err(AtlasError::Source(_))));
        fs::write(&case, &original).expect("reset case");
        edit_json(&case, |value| {
            value["votes"][0]["decision_ref"]["value"] = json!("70385");
        });
        assert!(matches!(inspect(&case), Err(AtlasError::Source(_))));
        fs::write(&case, &original).expect("reset case");
        edit_json(&case, |value| {
            value["contract_link"]["state"] = json!("executed");
        });
        assert!(matches!(inspect(&case), Err(AtlasError::Manifest(_))));
        let _ = dir;
    }

    #[test]
    fn truncated_scalars_omitted_votes_and_wrong_jurisdiction_fail() {
        let (_dir, case) = copy_case("commerce-city-ord-2755-2026-04-06.v2.json");
        let original = fs::read(&case).expect("original manifest");
        for (field, shortened) in [
            ("official_vote_id", "3847"),
            ("recorded_vote", "Na"),
            ("person_name", "Steven J."),
        ] {
            edit_json(&case, |value| {
                value["votes"][0][field]["value"] = json!(shortened);
                value["votes"][0][field]["proof"]["quote"] = json!(shortened);
            });
            assert!(inspect(&case).is_err(), "truncated {field} passed");
            fs::write(&case, &original).expect("reset manifest");
        }
        edit_json(&case, |value| {
            value["votes"].as_array_mut().expect("votes").pop();
        });
        assert!(inspect(&case).is_err(), "omitted Nay row passed");
        fs::write(&case, &original).expect("reset manifest");
        edit_json(&case, |value| {
            value["votes"] = json!([]);
        });
        assert!(inspect(&case).is_err(), "deleted entire roll call passed");
        fs::write(&case, &original).expect("reset manifest");
        edit_json(&case, |value| {
            value["jurisdiction"]["jurisdiction_ref"] = json!("us-co-place-0820000");
        });
        assert!(
            matches!(inspect(&case), Err(AtlasError::Source(_))),
            "Commerce case mapped to Denver"
        );
        fs::write(&case, &original).expect("reset manifest");
        edit_json(&case, |value| {
            value["jurisdiction"]["map_anchor"] = json!({"lat":39.7,"lon":-104.9});
        });
        assert!(inspect(&case).is_err(), "arbitrary map point passed");
    }

    #[test]
    fn source_verified_release_respects_explicit_human_veto_and_blob_integrity() {
        let (dir, case) = copy_case("commerce-city-ord-2755-2026-04-06.v2.json");
        let store = Store::open(dir.path().join("db")).expect("store");
        allow_release(&store);
        seed_receipts(&store, std::slice::from_ref(&case));
        let checked = inspect(&case).expect("case");
        let first_source = checked.sources.values().next().expect("source");
        let blob = store
            .content_path(&first_source.registered.sha256)
            .expect("blob path");
        let original_blob = fs::read(&blob).expect("blob");
        fs::write(&blob, b"corrupted private blob").expect("corrupt blob");
        let output = dir.path().join("public.json");
        assert!(matches!(
            super::super::export_atlas(
                &store,
                &case,
                &output,
                AtlasExportMode::Production,
                Some("https://panopticon.fail")
            ),
            Err(AtlasError::Review(_))
        ));
        fs::write(&blob, original_blob).expect("restore blob");
        let req = requirement(&checked);
        let rejected = ReviewDecision {
            id: ReviewDecision::id_for(&req.id, "2026-11-10T13:00:00Z"),
            citation_id: req.id.clone(),
            state: ReviewState::Rejected,
            reviewer: "Ada Editor".to_owned(),
            note: "Incorrect interpretation".to_owned(),
            bound_digest: req.bound_digest.clone(),
            decision_digest: String::new(),
            decided_at: "2026-11-10T13:00:00Z".to_owned(),
            supersedes: None,
        };
        store.insert_review(&rejected).expect("human veto");
        assert!(matches!(
            super::super::export_atlas(
                &store,
                &case,
                &output,
                AtlasExportMode::Production,
                Some("https://panopticon.fail")
            ),
            Err(AtlasError::Review(_))
        ));
        assert!(!output.exists());
    }

    #[test]
    fn newer_changed_capture_blocks_stale_release_but_304_preserves_it() {
        let (dir, case) = copy_case("commerce-city-ord-2755-2026-04-06.v2.json");
        let store = Store::open(dir.path().join("db")).expect("store");
        allow_release(&store);
        seed_receipts(&store, std::slice::from_ref(&case));
        let checked = inspect(&case).expect("case");
        let source = checked
            .sources
            .values()
            .find(|source| source.registered.id == "co-commerce-2026-event")
            .expect("event source");
        let registered = &source.registered;
        let acquisition_id = registered
            .acquisition_source_id
            .as_deref()
            .expect("archive ID");
        // A renewed policy review may be followed by a conditional fetch.
        // The 304 must inherit the archived 200 digest from before renewal,
        // while the current review still has to authorize a fresh receipt.
        let mut renewed = store
            .current_source_review(acquisition_id)
            .expect("review query")
            .expect("existing review");
        renewed.supersedes = Some(renewed.id.clone());
        renewed.reviewed_at = "2026-11-10T12:30:00Z".to_owned();
        renewed.id = SourceReview::id_for(acquisition_id, &renewed.reviewed_at);
        renewed.expires_at = "2099-01-01".to_owned();
        store
            .insert_source_review(&renewed)
            .expect("renewed date-only source review");
        let later_304 = FetchObservation {
            id: FetchObservation::id_for(&registered.url, "2026-11-10T13:00:00Z", 304),
            source_id: Some(acquisition_id.to_owned()),
            requested_url: registered.url.clone(),
            resolved_ips: vec!["1.1.1.1".to_owned()],
            retrieved_at: "2026-11-10T13:00:00Z".to_owned(),
            method: "GET".to_owned(),
            status_code: 304,
            redirect_target: None,
            final_url: registered.url.clone(),
            allowlisted_headers: vec![],
            content_type: None,
            content_length: None,
            etag: None,
            last_modified: None,
            body_digest: None,
            error: None,
        };
        store
            .insert_fetch_observation(&later_304)
            .expect("304 observation");
        let output = dir.path().join("public.json");
        super::super::export_atlas(
            &store,
            &case,
            &output,
            AtlasExportMode::Production,
            Some("https://panopticon.fail"),
        )
        .expect("304 inherits preserved bytes");
        let changed_200 = FetchObservation {
            id: FetchObservation::id_for(&registered.url, "2026-11-10T14:00:00Z", 200),
            source_id: Some(acquisition_id.to_owned()),
            requested_url: registered.url.clone(),
            resolved_ips: vec!["1.1.1.1".to_owned()],
            retrieved_at: "2026-11-10T14:00:00Z".to_owned(),
            method: "GET".to_owned(),
            status_code: 200,
            redirect_target: None,
            final_url: registered.url.clone(),
            allowlisted_headers: vec![],
            content_type: None,
            content_length: Some(9),
            etag: None,
            last_modified: None,
            body_digest: Some("b".repeat(64)),
            error: None,
        };
        store
            .insert_fetch_observation(&changed_200)
            .expect("changed observation");
        let previous = fs::read(&output).expect("previous output");
        assert!(matches!(
            super::super::export_atlas(
                &store,
                &case,
                &output,
                AtlasExportMode::Production,
                Some("https://panopticon.fail")
            ),
            Err(AtlasError::Review(_))
        ));
        assert_eq!(fs::read(&output).expect("untouched output"), previous);
    }

    #[test]
    fn event_and_vote_urls_require_exact_resource_paths() {
        let (dir, case) = copy_case("commerce-city-ord-2755-2026-04-06.v2.json");
        let registry = dir.path().join(REGISTRY_PATH);
        let original = fs::read(&registry).expect("registry");
        edit_json(&registry, |value| {
            let source = value["sources"]
                .as_array_mut()
                .expect("sources")
                .iter_mut()
                .find(|item| item["id"] == "co-commerce-2026-event")
                .expect("event source");
            source["url"] = json!(
                "https://webapi.legistar.com/v1/commerce/Events/46260?EventItems=1&AgendaNote=1&MinutesNote=1&EventItemAttachments=1"
            );
        });
        assert!(inspect(&case).is_err(), "event URL prefix spoof passed");
        fs::write(&registry, &original).expect("restore registry");
        edit_json(&registry, |value| {
            let source = value["sources"]
                .as_array_mut()
                .expect("sources")
                .iter_mut()
                .find(|item| item["id"] == "co-commerce-2026-ord2755-votes")
                .expect("vote source");
            source["url"] =
                json!("https://webapi.legistar.com/v1/commerce/EventItems/70384/VotesExtra");
        });
        assert!(inspect(&case).is_err(), "vote URL suffix spoof passed");
    }

    #[test]
    fn source_bytes_url_path_and_binary_claim_tampering_fail_closed() {
        let (dir, case) = copy_case("commerce-city-ord-2755-2026-04-06.v2.json");
        let output = dir.path().join("public.json");
        let store = Store::open(dir.path().join("db")).expect("store");
        allow_release(&store);
        fs::write(&output, b"previous safe snapshot").expect("old output");
        let event = dir
            .path()
            .join("fixtures/co-commerce/event-4626-2026-04-06.json");
        let original_event = fs::read(&event).expect("event");
        fs::write(&event, b"{}\n").expect("tampered event");
        assert!(matches!(inspect(&case), Err(AtlasError::Source(_))));
        assert!(
            super::super::export_atlas(
                &store,
                &case,
                &output,
                AtlasExportMode::Production,
                Some("https://panopticon.fail")
            )
            .is_err()
        );
        assert_eq!(
            fs::read(&output).expect("old output"),
            b"previous safe snapshot"
        );
        fs::write(&event, original_event).expect("restore event");
        let registry = dir.path().join(REGISTRY_PATH);
        let original_registry = fs::read(&registry).expect("registry");
        edit_json(&registry, |value| {
            value["sources"][9]["url"] = json!("https://attacker.example/v1/commerce/Events/4626");
        });
        assert!(matches!(inspect(&case), Err(AtlasError::Source(_))));
        fs::write(&registry, &original_registry).expect("restore registry");
        edit_json(&registry, |value| {
            value["sources"][9]["fixture"] = json!("fixtures/co-commerce/../../secret.json");
        });
        assert!(matches!(inspect(&case), Err(AtlasError::Source(_))));
        fs::write(&registry, &original_registry).expect("restore registry");
        edit_json(&case, |value| {
            value["technology_context"][0]["name"]["proof"] = json!({
                "source_id":"co-commerce-2026-ordinance","locator":"PDF p. 1",
                "quote":"LICENSE PLATE RECOGNITION CAMERAS"
            });
        });
        assert!(matches!(inspect(&case), Err(AtlasError::Source(_))));
    }

    #[test]
    fn html_locator_requires_one_exactly_quoted_element() {
        let source = VerifiedSource {
            registered: RegisteredSource {
                id: "official-test".to_owned(),
                acquisition_source_id: None,
                acquisition_config_digest: None,
                authority: "Test Council".to_owned(),
                url: "https://commerce.legistar.com/LegislationDetail.aspx".to_owned(),
                fixture: "fixtures/co-commerce/test.html".to_owned(),
                sha256: String::new(),
                title: "Test".to_owned(),
                format: "html".to_owned(),
            },
            bytes: b"<main><h1>Commerce City Council</h1><p>Other item</p></main>".to_vec(),
        };
        let good = Fact {
            value: "Commerce City".to_owned(),
            proof: ProofRef {
                source_id: "official-test".to_owned(),
                locator: "main h1".to_owned(),
                quote: "Commerce City Council".to_owned(),
            },
        };
        verify_fact("jurisdiction", &good, &source).expect("exact CSS quote");
        let bad = Fact {
            proof: ProofRef {
                locator: "main".to_owned(),
                ..good.proof.clone()
            },
            ..good.clone()
        };
        assert!(matches!(
            verify_fact("jurisdiction", &bad, &source),
            Err(AtlasError::Source(_))
        ));
        let ambiguous = Fact {
            proof: ProofRef {
                locator: "main *".to_owned(),
                ..good.proof.clone()
            },
            ..good
        };
        assert!(matches!(
            verify_fact("jurisdiction", &ambiguous, &source),
            Err(AtlasError::Source(_))
        ));
    }

    #[test]
    fn bundle_is_order_independent_and_rejects_duplicate_cases() {
        let cases = vec![
            manifest("commerce-city-ord-2755-2026-04-06.v2.json"),
            manifest("commerce-city-ord-2756-2026-04-06.v2.json"),
            manifest("castle-rock-res-2025-071.v2.json"),
            manifest("denver-25-0413-2025-05-05.v2.json"),
        ];
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::open(dir.path().join("db")).expect("store");
        let one = dir.path().join("one.json");
        let two = dir.path().join("two.json");
        super::super::export_atlas_bundle(&store, &cases, &one, AtlasExportMode::Preview, None)
            .expect("first bundle");
        let reversed = cases.iter().rev().cloned().collect::<Vec<_>>();
        super::super::export_atlas_bundle(&store, &reversed, &two, AtlasExportMode::Preview, None)
            .expect("reversed bundle");
        assert_eq!(
            fs::read(&one).expect("first bytes"),
            fs::read(&two).expect("second bytes")
        );
        let snapshot: Value =
            serde_json::from_slice(&fs::read(&one).expect("preview")).expect("snapshot JSON");
        assert_eq!(
            snapshot["case_releases"]
                .as_array()
                .expect("case releases")
                .len(),
            4
        );
        assert_eq!(
            snapshot["entities"]
                .as_array()
                .expect("entities")
                .iter()
                .filter(|entity| entity["type"] == "JURISDICTION")
                .count(),
            3
        );
        assert_eq!(
            snapshot["entities"]
                .as_array()
                .expect("entities")
                .iter()
                .filter(|entity| entity["type"] == "DECISION")
                .count(),
            4
        );
        let duplicate = vec![cases[0].clone(), cases[0].clone()];
        assert!(matches!(
            super::super::export_atlas_bundle(
                &store,
                &duplicate,
                &one,
                AtlasExportMode::Preview,
                None
            ),
            Err(AtlasError::Manifest(_))
        ));
        assert_eq!(
            fs::read(&one).expect("untouched bundle"),
            fs::read(&two).expect("second bundle")
        );
    }

    #[test]
    fn same_portal_person_id_links_two_decisions_without_name_only_merging() {
        let cases = vec![
            manifest("commerce-city-ord-2755-2026-04-06.v2.json"),
            manifest("commerce-city-ord-2756-2026-04-06.v2.json"),
        ];
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::open(dir.path().join("db")).expect("store");
        let output = dir.path().join("preview.json");
        super::super::export_atlas_bundle(&store, &cases, &output, AtlasExportMode::Preview, None)
            .expect("two Commerce decisions");
        let snapshot: Value =
            serde_json::from_slice(&fs::read(output).expect("preview")).expect("snapshot JSON");
        let entities = snapshot["entities"].as_array().expect("entities");
        let voters = entities
            .iter()
            .filter(|entity| entity["type"] == "RECORDED_VOTER")
            .collect::<Vec<_>>();
        assert_eq!(voters.len(), 9);
        let steven = voters
            .iter()
            .find(|entity| entity["name"] == "Steven J. Douglas")
            .expect("same portal person record");
        assert_eq!(steven["identity_scope"], "legistar:commerce");
        assert_eq!(steven["official_person_id"], "210");
        assert!(
            steven["unknowns"]
                .as_array()
                .expect("unknowns")
                .iter()
                .any(|gap| gap["kind"] == "office_term")
        );
        assert_eq!(steven["timeline"].as_array().expect("timeline").len(), 2);
        assert_eq!(steven["source_ids"].as_array().expect("sources").len(), 2);
        assert!(
            entities
                .iter()
                .filter(|entity| entity["type"] == "DECISION")
                .all(|decision| decision["votes"]
                    .as_array()
                    .expect("votes")
                    .iter()
                    .any(|vote| vote["voter_entity_id"] == steven["id"]))
        );
        for (field, value) in [
            ("identity_scope", json!("legistar:unrelated-client")),
            ("name", json!("Different Person")),
            ("jurisdiction_ref", json!("us-co-place-0000000")),
        ] {
            let mut forged = (**steven).clone();
            forged[field] = value;
            let mut original = (**steven).clone();
            assert!(matches!(
                super::super::merge_shared_entity(&mut original, &forged),
                Err(AtlasError::Manifest(_))
            ));
        }
    }

    #[test]
    fn source_verified_bundle_production_needs_only_global_allowlist() {
        let cases = vec![
            manifest("commerce-city-ord-2755-2026-04-06.v2.json"),
            manifest("denver-25-0413-2025-05-05.v2.json"),
        ];
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::open(dir.path().join("db")).expect("store");
        let output = dir.path().join("public.json");
        allow_release(&store);
        seed_receipts(&store, &cases);
        super::super::export_atlas_bundle(
            &store,
            &cases,
            &output,
            AtlasExportMode::Production,
            Some("https://panopticon.fail"),
        )
        .expect("source verified bundle");
        let snapshot: Value =
            serde_json::from_slice(&fs::read(&output).expect("public bytes")).expect("public JSON");
        assert_eq!(snapshot["release_review"]["state"], "SOURCE_VERIFIED");
        assert!(snapshot["release_review"]["reviewed_at"].is_null());
        assert_eq!(
            snapshot["case_releases"]
                .as_array()
                .expect("case releases")
                .len(),
            2
        );
        assert!(
            snapshot["case_releases"]
                .as_array()
                .expect("case releases")
                .iter()
                .all(|case| case["release"]["state"] == "SOURCE_VERIFIED")
        );
    }
}
