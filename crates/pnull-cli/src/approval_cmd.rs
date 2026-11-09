//! A narrow, offline import of one documented council approval.
//!
//! The imported links start Pending. The operator must inspect exact preserved
//! source bytes and approve each binding before the public chain includes it.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use pnull_core::{
    ApprovalCorrection, ApprovalCorrectionKind, ApprovalGap, ApprovalGapKind, ApprovalLink,
    ApprovalLinkKind, ApprovalReview, CivicDecision, ContractRecord, ContractStatus, DecisionKind,
    DecisionOutcome, DecisionParticipation, OfficeTerm, ParticipationKind, ReviewState,
    SourceProof, Store, TechnologyRecord, sha256_hex,
};
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

const JURISDICTION: &str = "Chino, California";
const IMPORTED_AT: &str = "2026-11-09T07:15:00Z";
const MATTER_URL: &str = "https://webapi.legistar.com/v1/chino/Matters/3139";
const ITEM_URL: &str = "https://webapi.legistar.com/v1/chino/Events/1961/EventItems/52243?AgendaNote=1&MinutesNote=1&Attachments=1";
const CONSENT_URL: &str = "https://webapi.legistar.com/v1/chino/Events/1961/EventItems/53661";
const VOTES_URL: &str = "https://webapi.legistar.com/v1/chino/EventItems/53661/Votes";

#[derive(Deserialize)]
struct Matter {
    #[serde(rename = "MatterId")]
    id: u64,
    #[serde(rename = "MatterFile")]
    file: String,
    #[serde(rename = "MatterName")]
    name: String,
    #[serde(rename = "MatterTitle")]
    title: String,
    #[serde(rename = "MatterBodyName")]
    body: String,
    #[serde(rename = "MatterStatusName")]
    status: String,
    #[serde(rename = "MatterPassedDate")]
    passed_at: String,
}

#[derive(Deserialize)]
struct Event {
    #[serde(rename = "EventId")]
    id: u64,
    #[serde(rename = "EventBodyName")]
    body: String,
    #[serde(rename = "EventDate")]
    date: String,
}

#[derive(Deserialize)]
struct EventItem {
    #[serde(rename = "EventItemId")]
    id: u64,
    #[serde(rename = "EventItemEventId")]
    event_id: u64,
    #[serde(rename = "EventItemMatterId")]
    matter_id: u64,
    #[serde(rename = "EventItemMatterFile")]
    matter_file: String,
    #[serde(rename = "EventItemMatterName")]
    matter_name: String,
    #[serde(rename = "EventItemAgendaNumber")]
    agenda_number: String,
    #[serde(rename = "EventItemConsent")]
    consent: u8,
    #[serde(rename = "EventItemActionName")]
    action: String,
    #[serde(rename = "EventItemPassedFlagName")]
    passed: String,
}

#[derive(Deserialize)]
struct ConsentItem {
    #[serde(rename = "EventItemId")]
    id: u64,
    #[serde(rename = "EventItemEventId")]
    event_id: u64,
    #[serde(rename = "EventItemActionName")]
    action: String,
    #[serde(rename = "EventItemActionText")]
    action_text: String,
    #[serde(rename = "EventItemPassedFlagName")]
    passed: String,
    #[serde(rename = "EventItemMover")]
    mover: String,
    #[serde(rename = "EventItemSeconder")]
    seconder: String,
}

struct OfficeProfile {
    person_name: &'static str,
    profile_name: &'static str,
    person_id: u64,
    fixture: &'static str,
    url: &'static str,
    role: &'static str,
    term: &'static str,
    email: &'static str,
    role_locator: &'static str,
}

const OFFICE_PROFILES: [OfficeProfile; 5] = [
    OfficeProfile {
        person_name: "Eunice M. Ulloa",
        profile_name: "Eunice M. Ulloa",
        person_id: 264,
        fixture: "chino-office-ulloa.html",
        url: "https://cityofchino.org/m/directory/employee?eid=40",
        role: "Mayor",
        term: "2024 to 2028",
        email: "EUlloa@cityofchino.org",
        role_locator: "h3.directory-subhead",
    },
    OfficeProfile {
        person_name: "Curtis Burton",
        profile_name: "Curtis Burton",
        person_id: 260,
        fixture: "chino-office-burton.html",
        url: "https://cityofchino.org/580/Mayor-Pro-Tem-Curtis-Burton",
        role: "Mayor Pro Tem, District 2",
        term: "2022-2026",
        email: "cburton@cityofchino.org",
        role_locator: "div.field.p-job-title",
    },
    OfficeProfile {
        person_name: "Karen C. Comstock",
        profile_name: "Karen Comstock",
        person_id: 267,
        fixture: "chino-office-comstock.html",
        url: "https://cityofchino.org/518/Karen-Comstock",
        role: "Council Member, District 4",
        term: "2024 to 2028",
        email: "KComstock@cityofchino.org",
        role_locator: "div.field.p-job-title",
    },
    OfficeProfile {
        person_name: "Christopher Flores",
        profile_name: "Christopher Flores",
        person_id: 239,
        fixture: "chino-office-flores.html",
        url: "https://cityofchino.org/519/Christopher-Flores",
        role: "Council Member, District 1",
        term: "2024 to 2028",
        email: "CFlores@cityofchino.org",
        role_locator: "div.field.p-job-title",
    },
    OfficeProfile {
        person_name: "Marc Lucio",
        profile_name: "Marc Lucio",
        person_id: 221,
        fixture: "chino-office-lucio.html",
        url: "https://cityofchino.org/517/Marc-Lucio",
        role: "Council Member, District 3",
        term: "2022 to 2026",
        email: "MLucio@cityofchino.org",
        role_locator: "div.field.p-job-title",
    },
];

#[derive(Deserialize)]
struct Vote {
    #[serde(rename = "VoteId")]
    id: u64,
    #[serde(rename = "VoteEventItemId")]
    event_item_id: u64,
    #[serde(rename = "VotePersonId")]
    person_id: u64,
    #[serde(rename = "VotePersonName")]
    person_name: String,
    #[serde(rename = "VoteValueName")]
    value: String,
}

#[derive(Deserialize)]
struct Person {
    #[serde(rename = "PersonId")]
    id: u64,
    #[serde(rename = "PersonFullName")]
    full_name: String,
    #[serde(rename = "PersonEmail")]
    email: String,
}

struct VerifiedSource {
    bytes: Vec<u8>,
    digest: String,
}

fn read_verified(
    dir: &Path,
    manifest: &BTreeMap<String, String>,
    name: &str,
) -> Result<VerifiedSource> {
    let expected = manifest
        .get(name)
        .with_context(|| format!("missing {name} from SHA256SUMS"))?;
    let bytes = fs::read(dir.join(name)).with_context(|| format!("read {name}"))?;
    let digest = sha256_hex(&bytes);
    ensure!(&digest == expected, "fixture digest mismatch for {name}");
    Ok(VerifiedSource { bytes, digest })
}

fn read_manifest(dir: &Path) -> Result<BTreeMap<String, String>> {
    let content = fs::read_to_string(dir.join("SHA256SUMS"))?;
    let mut entries = BTreeMap::new();
    for line in content.lines() {
        let (digest, name) = line
            .split_once("  ")
            .with_context(|| format!("invalid SHA256SUMS line: {line}"))?;
        ensure!(
            digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid digest for {name}"
        );
        ensure!(
            !name.contains('/') && !name.contains('\\'),
            "unsafe fixture name {name}"
        );
        ensure!(
            entries.insert(name.to_owned(), digest.to_owned()).is_none(),
            "duplicate fixture {name}"
        );
    }
    Ok(entries)
}

fn verify_office_profile(source: &VerifiedSource, profile: &OfficeProfile) -> Result<()> {
    let html = std::str::from_utf8(&source.bytes)?;
    ensure!(
        html.contains(profile.profile_name),
        "office source changed person name: {}",
        profile.person_name
    );
    ensure!(
        html.contains(&format!(">{}</", profile.role)),
        "office source changed role: {}",
        profile.person_name
    );
    ensure!(
        html.contains(&format!("mailto:{}", profile.email)),
        "office source changed public contact: {}",
        profile.person_name
    );
    let term_heading = html
        .find("Term of Office</h2>")
        .with_context(|| format!("office source lost term heading: {}", profile.person_name))?;
    let following = &html[term_heading..html.len().min(term_heading + 650)];
    ensure!(
        following.contains(&format!(">{}</p>", profile.term)),
        "office source changed term: {}",
        profile.person_name
    );
    Ok(())
}

fn proof(url: &str, digest: &str, locator: &str, quote: &str) -> SourceProof {
    SourceProof {
        source_url: url.to_owned(),
        snapshot_digest: digest.to_owned(),
        locator: locator.to_owned(),
        quote: quote.to_owned(),
    }
}

fn pending_link(store: &Store, mut link: ApprovalLink) -> Result<()> {
    link.id = ApprovalLink::id_for(
        link.kind,
        &link.from_id,
        &link.to_id,
        &link.proof,
        &link.supporting_proofs,
    );
    store.insert_approval_link(&link)?;
    let review = ApprovalReview {
        id: ApprovalReview::id_for(&link.id, IMPORTED_AT),
        link_id: link.id.clone(),
        state: ReviewState::Pending,
        bound_digest: link.binding_digest(),
        reviewer: "fixture import".to_owned(),
        note: "Source fields checked by importer; human review required for publication".to_owned(),
        decided_at: IMPORTED_AT.to_owned(),
        supersedes: None,
    };
    store.insert_approval_review(&review)?;
    Ok(())
}

/// Import exact Chino Legistar and City profile responses as a draft chain.
/// Returns the stable contract identifier for subsequent review and inspection.
#[allow(clippy::too_many_lines)] // One fixed, cross-checked source bundle; keep the joins visible together.
pub fn import_chino_case(store: &Store, fixtures_dir: &Path) -> Result<String> {
    let manifest = read_manifest(fixtures_dir)?;
    let matter_source = read_verified(fixtures_dir, &manifest, "chino-matter-26-308.json")?;
    let event_source = read_verified(fixtures_dir, &manifest, "chino-event-1961.json")?;
    let item_source = read_verified(fixtures_dir, &manifest, "chino-event-item-52243.json")?;
    let consent_source = read_verified(fixtures_dir, &manifest, "chino-consent-item-53661.json")?;
    let votes_source = read_verified(fixtures_dir, &manifest, "chino-votes-53661.json")?;
    let office_sources: Vec<VerifiedSource> = OFFICE_PROFILES
        .iter()
        .map(|profile| read_verified(fixtures_dir, &manifest, profile.fixture))
        .collect::<Result<_>>()?;
    let person_sources: Vec<VerifiedSource> = OFFICE_PROFILES
        .iter()
        .map(|profile| {
            read_verified(
                fixtures_dir,
                &manifest,
                &format!("chino-person-{}.json", profile.person_id),
            )
        })
        .collect::<Result<_>>()?;

    let matter: Matter = serde_json::from_slice(&matter_source.bytes)?;
    let event: Event = serde_json::from_slice(&event_source.bytes)?;
    let item: EventItem = serde_json::from_slice(&item_source.bytes)?;
    let consent: ConsentItem = serde_json::from_slice(&consent_source.bytes)?;
    let votes: Vec<Vote> = serde_json::from_slice(&votes_source.bytes)?;

    ensure!(
        matter.id == 3139 && matter.file == "26-308",
        "unexpected matter identity"
    );
    ensure!(
        matter.body == "City Council" && matter.status == "Passed",
        "matter is not a passed council item"
    );
    ensure!(
        matter.passed_at.starts_with("2026-06-16"),
        "unexpected passage date"
    );
    ensure!(
        matter
            .name
            .contains("Approve an agreement with Axon Enterprise Inc."),
        "matter does not identify an Axon agreement"
    );
    ensure!(
        matter.name.contains("Axon Basic with Full AI"),
        "matter does not identify the technology package"
    );
    ensure!(
        matter.name.contains("$4,464,008.92") && matter.name.contains("5-year"),
        "matter amount or term changed"
    );
    ensure!(
        event.id == 1961 && event.body == "City Council" && event.date.starts_with("2026-06-16"),
        "unexpected meeting identity"
    );
    ensure!(
        item.id == 52243
            && item.event_id == event.id
            && item.matter_id == matter.id
            && item.matter_file == matter.file
            && item.matter_name == matter.name,
        "event item is not this matter"
    );
    ensure!(
        item.agenda_number == "9." && item.consent == 1,
        "Axon item is not consent item 9"
    );
    ensure!(
        item.action == "approved" && item.passed == "Pass",
        "Axon item does not record approval"
    );
    ensure!(
        consent.id == 53661
            && consent.event_id == event.id
            && consent.action == "approved the Consent Agenda"
            && consent.passed == "Pass",
        "consent motion is not the passed meeting motion"
    );
    ensure!(
        consent
            .action_text
            .contains("approve the Consent Calendar items 1-14 as presented"),
        "consent motion no longer includes item 9"
    );
    ensure!(votes.len() == 5, "expected five published individual votes");
    let mut person_ids = BTreeSet::new();
    let mut vote_ids = BTreeSet::new();
    for vote in &votes {
        ensure!(
            vote.event_item_id == consent.id && vote.value == "AYES",
            "unexpected consent-calendar vote"
        );
        ensure!(
            !vote.person_name.trim().is_empty(),
            "vote has no person name"
        );
        ensure!(
            person_ids.insert(vote.person_id) && vote_ids.insert(vote.id),
            "duplicate person or vote"
        );
        ensure!(
            OFFICE_PROFILES
                .iter()
                .any(|profile| profile.person_name == vote.person_name
                    && profile.person_id == vote.person_id),
            "vote lacks a documented office profile"
        );
    }

    for (index, profile) in OFFICE_PROFILES.iter().enumerate() {
        verify_office_profile(&office_sources[index], profile)?;
        let person: Person = serde_json::from_slice(&person_sources[index].bytes)?;
        ensure!(
            person.id == profile.person_id
                && person.full_name == profile.person_name
                && person.email.eq_ignore_ascii_case(profile.email),
            "Legistar person and city office profile do not match: {}",
            profile.person_name
        );
    }

    for source in [
        &matter_source,
        &event_source,
        &item_source,
        &consent_source,
        &votes_source,
    ] {
        store.persist_content(&source.digest, &source.bytes)?;
    }
    for source in &office_sources {
        store.persist_content(&source.digest, &source.bytes)?;
    }
    for source in &person_sources {
        store.persist_content(&source.digest, &source.bytes)?;
    }

    let contract_proof = proof(
        MATTER_URL,
        &matter_source.digest,
        "$.MatterName",
        &matter.name,
    );
    let contract = ContractRecord {
        id: ContractRecord::id_for(JURISDICTION, &matter.file),
        procurement_matter_id: None,
        official_identifier: matter.file.clone(),
        title: matter.title.clone(),
        vendor: Some("Axon Enterprise Inc.".to_owned()),
        amount_raw: Some("$4,464,008.92".to_owned()),
        term_raw: Some("5-year agreement".to_owned()),
        status: ContractStatus::Authorized,
        proof: contract_proof.clone(),
    };
    store.insert_contract_record(&contract)?;

    let decision = CivicDecision {
        id: CivicDecision::id_for(
            JURISDICTION,
            "consent-calendar-2026-06-16",
            &consent.id.to_string(),
        ),
        jurisdiction: JURISDICTION.to_owned(),
        body: event.body,
        official_matter_id: "consent-calendar-2026-06-16".to_owned(),
        official_event_item_id: consent.id.to_string(),
        decided_on: "2026-06-16".to_owned(),
        subject: consent.action_text.clone(),
        kind: DecisionKind::LegislativeVote,
        outcome: DecisionOutcome::Approved,
        proof: proof(
            CONSENT_URL,
            &consent_source.digest,
            "$.EventItemActionText",
            &consent.action_text,
        ),
    };
    store.insert_civic_decision(&decision)?;

    let technology = TechnologyRecord {
        id: TechnologyRecord::id_for("Axon Basic with Full AI", "named agreement package"),
        name: "Axon Basic with Full AI".to_owned(),
        category: "named agreement package".to_owned(),
        vendor: Some("Axon Enterprise Inc.".to_owned()),
        proof: proof(
            MATTER_URL,
            &matter_source.digest,
            "$.MatterName",
            "Axon Basic with Full AI",
        ),
    };
    store.insert_technology_record(&technology)?;

    pending_link(store, ApprovalLink {
        id: String::new(),
        kind: ApprovalLinkKind::DecisionAuthorizesContract,
        from_id: decision.id.clone(),
        to_id: contract.id.clone(),
        proof: proof(ITEM_URL, &item_source.digest, "$.EventItemMatterName", &item.matter_name),
        supporting_proofs: vec![
            proof(ITEM_URL, &item_source.digest, "$.EventItemAgendaNumber", &item.agenda_number),
            proof(ITEM_URL, &item_source.digest, "$.EventItemConsent", "1"),
            proof(ITEM_URL, &item_source.digest, "$.EventItemActionName", &item.action),
            proof(ITEM_URL, &item_source.digest, "$.EventItemPassedFlagName", &item.passed),
            proof(CONSENT_URL, &consent_source.digest, "$.EventItemActionText", &consent.action_text),
            proof(CONSENT_URL, &consent_source.digest, "$.EventItemPassedFlagName", &consent.passed),
        ],
        note: "The recorded consent motion approved items 1-14. Axon matter 26-308 is consent item 9 and separately marked approved. Execution is not asserted.".to_owned(),
    })?;
    pending_link(
        store,
        ApprovalLink {
            id: String::new(),
            kind: ApprovalLinkKind::ContractIncludesTechnology,
            from_id: contract.id.clone(),
            to_id: technology.id,
            proof: technology.proof,
            supporting_proofs: Vec::new(),
            note: "The approved matter names this agreement package.".to_owned(),
        },
    )?;

    let mut office_by_name = BTreeMap::new();
    for (index, vote) in votes.iter().enumerate() {
        let profile_index = OFFICE_PROFILES
            .iter()
            .position(|profile| profile.person_name == vote.person_name)
            .with_context(|| format!("missing office profile for {}", vote.person_name))?;
        let profile = &OFFICE_PROFILES[profile_index];
        let office_source = &office_sources[profile_index];
        let person_source = &person_sources[profile_index];
        let person: Person = serde_json::from_slice(&person_source.bytes)?;
        let office = OfficeTerm {
            id: OfficeTerm::id_for(
                JURISDICTION,
                "City Council",
                &vote.person_id.to_string(),
                profile.term,
            ),
            person_name: vote.person_name.clone(),
            official_person_id: Some(vote.person_id.to_string()),
            office_title: profile.role.to_owned(),
            body: "City Council".to_owned(),
            jurisdiction: JURISDICTION.to_owned(),
            starts_on: None,
            ends_on: None,
            term_span_raw: Some(profile.term.to_owned()),
            proof: proof(
                profile.url,
                &office_source.digest,
                profile.role_locator,
                profile.role,
            ),
            term_proof: Some(proof(
                profile.url,
                &office_source.digest,
                "//h2[normalize-space()='Term of Office']/following-sibling::p[1]",
                profile.term,
            )),
        };
        store.insert_office_term(&office)?;
        office_by_name.insert(vote.person_name.clone(), office.id.clone());
        let participation = DecisionParticipation {
            id: DecisionParticipation::id_for(&decision.id, &office.id, ParticipationKind::VoteYes),
            decision_id: decision.id.clone(),
            office_term_id: office.id,
            kind: ParticipationKind::VoteYes,
            official_vote_id: Some(vote.id.to_string()),
            raw_vote_value: Some(vote.value.clone()),
            proof: proof(
                VOTES_URL,
                &votes_source.digest,
                &format!("$[{index}].VoteValueName"),
                &vote.value,
            ),
        };
        store.insert_decision_participation(&participation)?;
        pending_link(
            store,
            ApprovalLink {
                id: String::new(),
                kind: ApprovalLinkKind::ParticipationRecordedForDecision,
                from_id: participation.id.clone(),
                to_id: decision.id.clone(),
                proof: participation.proof,
                supporting_proofs: Vec::new(),
                note: format!(
                    "Legistar vote {} records {} as AYES on consent-calendar motion 53661, which included Axon item 9.",
                    vote.id, vote.person_name
                ),
            },
        )?;
        pending_link(
            store,
            ApprovalLink {
                id: String::new(),
                kind: ApprovalLinkKind::ParticipationByOfficeholder,
                from_id: participation.id.clone(),
                to_id: office_by_name[&vote.person_name].clone(),
                proof: proof(
                    VOTES_URL,
                    &votes_source.digest,
                    &format!("$[{index}].VotePersonName"),
                    &vote.person_name,
                ),
                supporting_proofs: vec![
                    proof(
                        &format!(
                            "https://webapi.legistar.com/v1/chino/Persons/{}",
                            profile.person_id
                        ),
                        &person_source.digest,
                        "$.PersonEmail",
                        &person.email,
                    ),
                    proof(
                        profile.url,
                        &office_source.digest,
                        "a[href^='mailto:']",
                        profile.email,
                    ),
                    office.proof,
                ],
                note: format!(
                    "Vote {} names {} by Legistar person ID; that person's public office email matches the City profile's public office email.",
                    vote.id, vote.person_name
                ),
            },
        )?;
    }

    for (kind, name, locator) in [
        (ParticipationKind::Moved, &consent.mover, "$.EventItemMover"),
        (
            ParticipationKind::Seconded,
            &consent.seconder,
            "$.EventItemSeconder",
        ),
    ] {
        let office_id = office_by_name
            .get(name)
            .with_context(|| format!("{name} does not have a published individual vote"))?;
        let participation = DecisionParticipation {
            id: DecisionParticipation::id_for(&decision.id, office_id, kind),
            decision_id: decision.id.clone(),
            office_term_id: office_id.clone(),
            kind,
            official_vote_id: None,
            raw_vote_value: None,
            proof: proof(CONSENT_URL, &consent_source.digest, locator, name),
        };
        store.insert_decision_participation(&participation)?;
        pending_link(
            store,
            ApprovalLink {
                id: String::new(),
                kind: ApprovalLinkKind::ParticipationRecordedForDecision,
                from_id: participation.id.clone(),
                to_id: decision.id.clone(),
                proof: participation.proof,
                supporting_proofs: Vec::new(),
                note: format!(
                    "Consent-calendar motion 53661 records {name} as {}.",
                    kind.label()
                ),
            },
        )?;
        let office = store.office_term(office_id)?;
        let profile_index = OFFICE_PROFILES
            .iter()
            .position(|profile| profile.person_name == name)
            .context("consent mover/seconder lacks office profile")?;
        let profile = &OFFICE_PROFILES[profile_index];
        let person_source = &person_sources[profile_index];
        let person: Person = serde_json::from_slice(&person_source.bytes)?;
        pending_link(
            store,
            ApprovalLink {
                id: String::new(),
                kind: ApprovalLinkKind::ParticipationByOfficeholder,
                from_id: participation.id,
                to_id: office_id.clone(),
                proof: proof(CONSENT_URL, &consent_source.digest, locator, name),
                supporting_proofs: vec![
                    proof(
                        &format!(
                            "https://webapi.legistar.com/v1/chino/Persons/{}",
                            profile.person_id
                        ),
                        &person_source.digest,
                        "$.PersonFullName",
                        &person.full_name,
                    ),
                    proof(
                        &format!(
                            "https://webapi.legistar.com/v1/chino/Persons/{}",
                            profile.person_id
                        ),
                        &person_source.digest,
                        "$.PersonEmail",
                        &person.email,
                    ),
                    proof(
                        profile.url,
                        &office.proof.snapshot_digest,
                        "a[href^='mailto:']",
                        profile.email,
                    ),
                    office.proof,
                ],
                note: format!(
                    "Consent-calendar motion names {name} as {}; that same name is tied to a documented City office profile through the recorded vote and public office email.",
                    kind.label()
                ),
            },
        )?;
    }

    let mut digests = vec![
        matter_source.digest,
        event_source.digest,
        item_source.digest,
        consent_source.digest,
        votes_source.digest,
    ];
    digests.extend(office_sources.into_iter().map(|source| source.digest));
    digests.extend(person_sources.into_iter().map(|source| source.digest));
    for (kind, detail) in [
        (
            ApprovalGapKind::OfficeTermDatesNotEstablished,
            "The City publishes each officeholder's role and term years, but these pages do not give exact start and end dates.",
        ),
        (
            ApprovalGapKind::ContractExecutionNotEstablished,
            "The council authorized the Axon agreement; an executed agreement has not been preserved in this case.",
        ),
        (
            ApprovalGapKind::PaymentNotEstablished,
            "The approved amount is not evidence that payment occurred.",
        ),
        (
            ApprovalGapKind::DeploymentNotEstablished,
            "These approval records do not establish when any product was deployed.",
        ),
    ] {
        store.insert_approval_gap(&ApprovalGap {
            id: ApprovalGap::id_for(&contract.id, kind, IMPORTED_AT),
            subject_id: contract.id.clone(),
            kind,
            checked_source_digests: digests.clone(),
            detail: detail.to_owned(),
            recorded_at: IMPORTED_AT.to_owned(),
        })?;
    }
    Ok(contract.id)
}

pub fn show_case(store: &Store, contract_id: &str) -> Result<()> {
    let contract = store.contract_record(contract_id)?;
    println!(
        "{} | {} | {:?}",
        contract.id, contract.title, contract.status
    );
    println!(
        "vendor: {} | amount: {} | term: {}",
        contract.vendor.as_deref().unwrap_or("not established"),
        contract.amount_raw.as_deref().unwrap_or("not established"),
        contract.term_raw.as_deref().unwrap_or("not established")
    );
    println!(
        "source: {} | SHA-256 {}",
        contract.proof.source_url, contract.proof.snapshot_digest
    );
    for link in store.approval_links_for_contract(contract_id)? {
        print_link(store, &link)?;
        if link.kind == ApprovalLinkKind::DecisionAuthorizesContract {
            let decision = store.civic_decision(&link.from_id)?;
            println!(
                "  decision: {} | {} | {} | {:?}",
                decision.official_event_item_id,
                decision.body,
                decision.decided_on,
                decision.outcome
            );
            println!("    subject: {}", decision.subject);
            for participation in store.decision_participations(&link.from_id)? {
                let term = store.office_term(&participation.office_term_id)?;
                println!(
                    "  participation: {} | {} | {:?} | office term {}",
                    term.person_name,
                    term.office_title,
                    participation.kind,
                    term.term_span_raw.as_deref().unwrap_or("not established")
                );
                if let Some(term_proof) = &term.term_proof {
                    println!(
                        "    term proof: {}#{} | SHA-256 {} | {}",
                        term_proof.source_url,
                        term_proof.locator,
                        term_proof.snapshot_digest,
                        term_proof.quote
                    );
                }
                for participant_link in store.approval_links_for_participation(&participation.id)? {
                    print_link(store, &participant_link)?;
                }
            }
        } else if link.kind == ApprovalLinkKind::ContractIncludesTechnology {
            let technology = store.technology_record(&link.to_id)?;
            println!(
                "  technology: {} | {}",
                technology.name, technology.category
            );
        }
    }
    for gap in store.approval_gaps(contract_id)? {
        println!("not established: {:?} — {}", gap.kind, gap.detail);
    }
    Ok(())
}

fn print_link(store: &Store, link: &ApprovalLink) -> Result<()> {
    let state = store.approval_link_review_state(&link.id)?;
    println!("link {} | {:?} | {:?}", link.id, link.kind, state);
    println!("  {} → {}", link.from_id, link.to_id);
    println!("  binding: {}", link.binding_digest());
    println!(
        "  proof: {}#{} | SHA-256 {}",
        link.proof.source_url, link.proof.locator, link.proof.snapshot_digest
    );
    println!("  quote: {}", link.proof.quote);
    for proof in &link.supporting_proofs {
        println!(
            "  supporting proof: {}#{} | SHA-256 {}",
            proof.source_url, proof.locator, proof.snapshot_digest
        );
        println!("    quote: {}", proof.quote);
    }
    for review in store.approval_reviews(&link.id)? {
        println!(
            "  review: {} | {:?} | {} | {} | supersedes {}",
            review.id,
            review.state,
            review.reviewer,
            review.decided_at,
            review.supersedes.as_deref().unwrap_or("none")
        );
        println!("    note: {}", review.note);
    }
    for correction in store.approval_corrections(&link.id)? {
        println!(
            "  correction: {} | {:?} | {} | {}",
            correction.id, correction.kind, correction.operator, correction.recorded_at
        );
        println!("    reason: {}", correction.reason);
        println!(
            "    proof: {}#{} | SHA-256 {}",
            correction.proof.source_url, correction.proof.locator, correction.proof.snapshot_digest
        );
        if let Some(replacement) = correction.replacement_link_id {
            println!("    replacement link: {replacement}");
        }
    }
    Ok(())
}

pub fn review_link(
    store: &Store,
    link_id: &str,
    binding: &str,
    decision: &str,
    reviewer: &str,
    note: &str,
) -> Result<()> {
    ensure!(
        !reviewer.trim().is_empty() && !note.trim().is_empty(),
        "reviewer and note are required"
    );
    let link = store.approval_link(link_id)?;
    ensure!(
        link.binding_digest() == binding,
        "binding digest changed; inspect the link again"
    );
    let state = match decision {
        "approve" => ReviewState::Approved,
        "reject" => ReviewState::Rejected,
        "needs-context" => ReviewState::NeedsContext,
        _ => bail!("decision must be approve, reject, or needs-context"),
    };
    let reviews = store.approval_reviews(link_id)?;
    let now = OffsetDateTime::now_utc().format(&Rfc3339)?;
    let review = ApprovalReview {
        id: ApprovalReview::id_for(link_id, &now),
        link_id: link_id.to_owned(),
        state,
        bound_digest: binding.to_owned(),
        reviewer: reviewer.to_owned(),
        note: note.to_owned(),
        decided_at: now,
        supersedes: reviews.last().map(|prior| prior.id.clone()),
    };
    store.insert_approval_review(&review)?;
    println!(
        "recorded {:?} review {} for link {}",
        state, review.id, link_id
    );
    Ok(())
}

pub fn correct_link(
    store: &Store,
    link_id: &str,
    kind: &str,
    operator: &str,
    reason: &str,
    proof: SourceProof,
    replacement_link_id: Option<String>,
) -> Result<()> {
    ensure!(
        !operator.trim().is_empty() && !reason.trim().is_empty(),
        "operator and reason are required"
    );
    let kind = match kind {
        "retracted" => ApprovalCorrectionKind::Retracted,
        "replaced" => ApprovalCorrectionKind::Replaced,
        "clarified" => ApprovalCorrectionKind::Clarified,
        _ => bail!("kind must be retracted, replaced, or clarified"),
    };
    let now = OffsetDateTime::now_utc().format(&Rfc3339)?;
    let correction = ApprovalCorrection {
        id: ApprovalCorrection::id_for(link_id, kind, &now),
        link_id: link_id.to_owned(),
        kind,
        replacement_link_id,
        operator: operator.to_owned(),
        reason: reason.to_owned(),
        recorded_at: now,
        proof,
    };
    store.insert_approval_correction(&correction)?;
    println!(
        "recorded {:?} correction {} for link {}",
        kind, correction.id, link_id
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::too_many_lines)] // Covers the complete imported consent chain and review history.
    fn chino_fixture_builds_pending_evidence_chain() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let dir = tempfile::tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        let contract_id =
            import_chino_case(&store, &root.join("fixtures/approval")).expect("import");
        let chain = store.approval_chain(&contract_id).expect("chain");
        assert!(
            chain.authorizing_decisions.is_empty(),
            "pending links must not publish"
        );
        assert_eq!(chain.gaps.len(), 4);
        let links = store
            .approval_links_for_contract(&contract_id)
            .expect("links");
        assert_eq!(links.len(), 2);
        let decision_link = links
            .iter()
            .find(|link| link.kind == ApprovalLinkKind::DecisionAuthorizesContract)
            .expect("decision link");
        assert_eq!(
            decision_link.supporting_proofs.len(),
            6,
            "consent-to-item join needs both sources and item status"
        );
        assert_eq!(
            store
                .approval_link_review_state(&decision_link.id)
                .expect("state"),
            Some(ReviewState::Pending)
        );
        assert!(
            store
                .approval_corrections(&decision_link.id)
                .expect("corrections")
                .is_empty()
        );
        let decision = store
            .civic_decision(&decision_link.from_id)
            .expect("decision");
        assert_eq!(
            decision.official_event_item_id, "53661",
            "votes belong to the consent motion"
        );
        let participations = store
            .decision_participations(&decision.id)
            .expect("participations");
        assert_eq!(
            participations.len(),
            7,
            "five votes plus mover and seconder"
        );
        for participation in &participations {
            let edges = store
                .approval_links_for_participation(&participation.id)
                .expect("edges");
            assert_eq!(
                edges.len(),
                2,
                "both participation and identity edges need review"
            );
            let term = store
                .office_term(&participation.office_term_id)
                .expect("office term");
            assert!(term.term_span_raw.is_some());
            assert!(
                term.starts_on.is_none() && term.ends_on.is_none(),
                "exact dates remain unknown"
            );
        }
        let participation_links = store
            .approval_links_for_decision(&decision_link.from_id)
            .expect("participation links");
        assert_eq!(
            participation_links
                .iter()
                .filter(|link| link.kind == ApprovalLinkKind::ParticipationRecordedForDecision)
                .count(),
            7
        );
        let votes_digest = sha256_hex(
            &fs::read(root.join("fixtures/approval/chino-votes-53661.json")).expect("votes source"),
        );
        let saved_votes =
            fs::read(store.content_path(&votes_digest).expect("blob path")).expect("saved votes");
        assert_eq!(
            saved_votes,
            fs::read(root.join("fixtures/approval/chino-votes-53661.json")).expect("votes source")
        );
        let imported_again =
            import_chino_case(&store, &root.join("fixtures/approval")).expect("repeat import");
        assert_eq!(contract_id, imported_again);
        review_link(
            &store,
            &decision_link.id,
            &decision_link.binding_digest(),
            "approve",
            "test reviewer",
            "checked consent item and motion",
        )
        .expect("review");
        assert_eq!(
            store
                .approval_reviews(&decision_link.id)
                .expect("reviews")
                .len(),
            2
        );
        correct_link(
            &store,
            &decision_link.id,
            "retracted",
            "test reviewer",
            "test correction of a reviewed link",
            decision_link.proof.clone(),
            None,
        )
        .expect("correction");
        assert_eq!(
            store
                .approval_corrections(&decision_link.id)
                .expect("corrections")
                .len(),
            1
        );
        assert_eq!(
            store
                .approval_link_review_state(&decision_link.id)
                .expect("state"),
            Some(ReviewState::Superseded)
        );
    }
}
