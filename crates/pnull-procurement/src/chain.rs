//! Deterministic procurement-chain construction: an ordered view of
//! `solicitation -> amendment -> award -> contract -> expenditure`.
//!
//! A link between two chain stages is created **only** when the evidence
//! supports it:
//! - the two records reference identifiers whose normalized forms match
//!   exactly (the deterministic `uppercase-alphanumeric-compact` rule), and
//! - both records resolve to an exact, immutable source snapshot with a valid
//!   SHA-256 digest.
//!
//! A link is never bound to "the newest snapshot for a source" — a historical
//! event is bound to the exact snapshot that was recorded when it was
//! ingested, so a newer snapshot can never retroactively change an existing
//! link's evidence. Similar-but-not-exact identifiers, incomplete identifiers,
//! and ambiguous candidates are **never** auto-linked; they are surfaced as
//! review suggestions for a human. A stage with no observed supporting record
//! is rendered as "Not observed in the checked sources." and is never presented
//! as proof that no record exists. If either endpoint of a candidate link lacks
//! digest-bound evidence, the link is not created and an explicit evidence gap
//! is rendered instead.

use std::collections::{BTreeMap, BTreeSet};

use pnull_core::{
    ProcurementEvent, ProcurementEventKind, ProcurementIdentifier, ProcurementMatter,
    ReconciliationItem, Store,
};
use thiserror::Error;

use crate::coverage::NOT_OBSERVED_PHRASING;
use crate::reconcile::{candidate_identifier_item, exact_identifier_match};

/// The ordered stages of the procurement chain.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ChainStage {
    Solicitation,
    Amendment,
    Award,
    Contract,
    Expenditure,
}

impl ChainStage {
    /// All stages in canonical chain order.
    pub const ALL: [ChainStage; 5] = [
        ChainStage::Solicitation,
        ChainStage::Amendment,
        ChainStage::Award,
        ChainStage::Contract,
        ChainStage::Expenditure,
    ];

    /// The canonical ordered index of a stage.
    pub fn index(self) -> usize {
        match self {
            Self::Solicitation => 0,
            Self::Amendment => 1,
            Self::Award => 2,
            Self::Contract => 3,
            Self::Expenditure => 4,
        }
    }

    /// The human label for a stage.
    pub fn label(self) -> &'static str {
        match self {
            Self::Solicitation => "solicitation",
            Self::Amendment => "amendment",
            Self::Award => "award",
            Self::Contract => "contract",
            Self::Expenditure => "expenditure",
        }
    }

    /// The chain stage an event kind belongs to, if any.
    ///
    /// Corrective records (`RecordCorrected` / `RecordRemoved`) and unknown
    /// kinds are not chain stages and map to `None`.
    pub fn from_event_kind(kind: ProcurementEventKind) -> Option<Self> {
        match kind {
            ProcurementEventKind::SolicitationPublished
            | ProcurementEventKind::QuestionsAndAnswersPublished
            | ProcurementEventKind::SubmissionDeadlineChanged => Some(Self::Solicitation),
            ProcurementEventKind::AmendmentPublished => Some(Self::Amendment),
            ProcurementEventKind::AwardAnnounced => Some(Self::Award),
            ProcurementEventKind::ContractExecuted | ProcurementEventKind::ContractAmended => {
                Some(Self::Contract)
            }
            ProcurementEventKind::ExpenditureReported => Some(Self::Expenditure),
            ProcurementEventKind::RecordCorrected
            | ProcurementEventKind::RecordRemoved
            | ProcurementEventKind::Unknown => None,
        }
    }
}

/// How a link between two stages is supported.
///
/// Only exact-identifier links are currently implemented. A link that an
/// official record explicitly states is NOT yet supported: no ingestion path
/// produces one, so the capability is not advertised.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkKind {
    /// The two records reference identifiers whose normalized forms match exactly.
    ExactIdentifier,
}

impl LinkKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::ExactIdentifier => "exact identifier match",
        }
    }
}

/// Digest-bound evidence for one endpoint of a chain link.
///
/// Every field is copied from the exact `SourceSnapshot` the endpoint event was
/// bound to at ingestion time; it is never derived from "the newest snapshot for
/// the source" and is never an `"unknown"` placeholder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainEvidence {
    /// The event this evidence supports.
    pub event_id: String,
    /// The source the event was observed from.
    pub source_id: String,
    /// The exact immutable snapshot id the event is bound to.
    pub snapshot_id: String,
    /// The SHA-256 of the exact persisted bytes of that snapshot.
    pub digest: String,
}

/// An accepted, evidence-backed link between two chain stages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainLink {
    pub from: ChainStage,
    pub to: ChainStage,
    pub kind: LinkKind,
    /// The normalized identifier shared by both linked records (exact links).
    pub normalized_identifier: Option<String>,
    /// A citation/summary describing the supporting evidence.
    pub citation: String,
    /// Digest-bound evidence for the earlier-stage endpoint.
    pub from_evidence: ChainEvidence,
    /// Digest-bound evidence for the later-stage endpoint.
    pub to_evidence: ChainEvidence,
}

/// A candidate cross-stage link that could not be accepted because one or both
/// endpoints lack digest-bound evidence. Rendered as an explicit gap; never
/// presented as a link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceGap {
    /// The normalized identifier the two events share.
    pub normalized_identifier: String,
    pub from_event_id: String,
    pub to_event_id: String,
    pub from_has_evidence: bool,
    pub to_has_evidence: bool,
}

/// One chain stage with its observed supporting events.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainStageObservation {
    pub stage: ChainStage,
    /// Events observed at this stage, in deterministic order.
    pub events: Vec<ProcurementEvent>,
}

/// The deterministic chain view for a single matter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainView {
    pub matter: ProcurementMatter,
    /// The five stages in canonical order, each with observed events.
    pub stages: Vec<ChainStageObservation>,
    /// Accepted, evidence-backed links between stages.
    pub links: Vec<ChainLink>,
    /// Candidate cross-stage links rejected for missing evidence.
    pub evidence_gaps: Vec<EvidenceGap>,
    /// Similar-but-not-exact identifiers surfaced for human review.
    pub review_suggestions: Vec<ReconciliationItem>,
    /// Stages with no observed supporting record.
    pub not_observed: Vec<ChainStage>,
}

#[derive(Debug, Error)]
pub enum ChainError {
    #[error("procurement matter {0} not found")]
    MatterNotFound(String),
    #[error("store operation failed: {0}")]
    Store(#[from] pnull_core::CoreError),
}

/// Builds the deterministic procurement chain for a matter from its existing
/// events and identifiers. Never invents a link; exact matches link, ambiguous
/// candidates are queued, and unobserved stages are marked `Not observed`.
pub fn build_chain(store: &Store, matter_id: &str) -> Result<ChainView, ChainError> {
    let matter = store
        .procurement_matter(matter_id)
        .map_err(|_| ChainError::MatterNotFound(matter_id.to_owned()))?;

    let events = store.procurement_events(matter_id)?;
    let identifiers = store.procurement_identifiers(matter_id)?;

    // Group events by chain stage (deterministic order).
    let mut stage_events: BTreeMap<ChainStage, Vec<ProcurementEvent>> = BTreeMap::new();
    for event in &events {
        if let Some(stage) = ChainStage::from_event_kind(event.kind) {
            stage_events.entry(stage).or_default().push(event.clone());
        }
    }

    let stages: Vec<ChainStageObservation> = ChainStage::ALL
        .iter()
        .map(|stage| {
            let mut observed = stage_events.get(stage).cloned().unwrap_or_default();
            observed.sort_by(|a, b| a.date.cmp(&b.date).then(a.id.cmp(&b.id)));
            ChainStageObservation {
                stage: *stage,
                events: observed,
            }
        })
        .collect();

    let not_observed: Vec<ChainStage> = stages
        .iter()
        .filter(|s| s.events.is_empty())
        .map(|s| s.stage)
        .collect();

    // Build an index from normalized identifier -> events, keyed by stage.
    let identifier_by_id: BTreeMap<String, ProcurementIdentifier> = identifiers
        .iter()
        .map(|i| (i.id.clone(), i.clone()))
        .collect();

    // normalized identifier -> (stage, event)
    let mut by_normalized: BTreeMap<String, Vec<(ChainStage, ProcurementEvent)>> = BTreeMap::new();
    for event in &events {
        let Some(stage) = ChainStage::from_event_kind(event.kind) else {
            continue;
        };
        for identifier_id in &event.identifier_ids {
            if let Some(identifier) = identifier_by_id.get(identifier_id)
                && let Some(normalized) = &identifier.normalized
            {
                by_normalized
                    .entry(normalized.clone())
                    .or_default()
                    .push((stage, event.clone()));
            }
        }
    }

    // Create exact cross-stage links only when both endpoints resolve to
    // digest-bound snapshots. Otherwise record an explicit evidence gap.
    let (links, gaps) = build_links_and_gaps(store, &by_normalized);

    // Surface similar-but-not-exact identifiers for human review. These are
    // never linked and are not persisted reconciliation items.
    let review_suggestions = similar_identifier_candidates(matter_id, &identifiers);

    Ok(ChainView {
        matter,
        stages,
        links,
        evidence_gaps: gaps,
        review_suggestions,
        not_observed,
    })
}

/// Builds cross-stage links from normalized-identifier occurrences, creating a
/// link only when both endpoints resolve to digest-bound snapshots. Pairs
/// lacking digest-bound evidence on either endpoint become explicit evidence
/// gaps instead of links.
fn build_links_and_gaps(
    store: &Store,
    by_normalized: &BTreeMap<String, Vec<(ChainStage, ProcurementEvent)>>,
) -> (Vec<ChainLink>, Vec<EvidenceGap>) {
    let mut links = Vec::new();
    let mut gaps = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for (normalized, occ) in by_normalized {
        for (i, (from_stage, from_event)) in occ.iter().enumerate() {
            for (to_stage, to_event) in occ.iter().skip(i + 1) {
                if from_stage == to_stage {
                    continue; // within a stage, not a chain link
                }
                let pair_key = format!("{}\0{}", from_event.id, to_event.id);
                if !seen.insert(pair_key) {
                    continue;
                }
                let from_evidence = resolve_event_evidence(store, from_event);
                let to_evidence = resolve_event_evidence(store, to_event);
                match (from_evidence, to_evidence) {
                    (Some(ev_from), Some(ev_to)) => {
                        let (from, to) = ordered_stages(*from_stage, *to_stage);
                        links.push(ChainLink {
                            from,
                            to,
                            kind: LinkKind::ExactIdentifier,
                            normalized_identifier: Some(normalized.clone()),
                            citation: format!(
                                "events '{}' (source {}) and '{}' (source {}) reference the same normalized identifier {normalized}",
                                from_event.summary,
                                from_event.source_id,
                                to_event.summary,
                                to_event.source_id
                            ),
                            from_evidence: ev_from,
                            to_evidence: ev_to,
                        });
                    }
                    (from_ev, to_ev) => {
                        gaps.push(EvidenceGap {
                            normalized_identifier: normalized.clone(),
                            from_event_id: from_event.id.clone(),
                            to_event_id: to_event.id.clone(),
                            from_has_evidence: from_ev.is_some(),
                            to_has_evidence: to_ev.is_some(),
                        });
                    }
                }
            }
        }
    }
    links.sort_by(|a, b| {
        a.from
            .index()
            .cmp(&b.from.index())
            .then(a.to.index().cmp(&b.to.index()))
            .then(a.normalized_identifier.cmp(&b.normalized_identifier))
    });
    gaps.sort_by(|a, b| {
        a.normalized_identifier
            .cmp(&b.normalized_identifier)
            .then(a.from_event_id.cmp(&b.from_event_id))
            .then(a.to_event_id.cmp(&b.to_event_id))
    });
    (links, gaps)
}

/// Orders two chain-stage endpoints into canonical chain order, returning the
/// earlier and later stages.
fn ordered_stages(a_stage: ChainStage, b_stage: ChainStage) -> (ChainStage, ChainStage) {
    if a_stage.index() <= b_stage.index() {
        (a_stage, b_stage)
    } else {
        (b_stage, a_stage)
    }
}

/// Resolves an event's digest-bound evidence from the exact snapshot it was
/// bound to at ingestion time (recorded in `evidence_ids`). Returns `None` when
/// no evidence id resolves to a stored snapshot with a valid SHA-256 digest.
fn resolve_event_evidence(store: &Store, event: &ProcurementEvent) -> Option<ChainEvidence> {
    for evidence_id in &event.evidence_ids {
        if let Ok(snapshot) = store.source_snapshot(evidence_id) {
            let digest = snapshot.persisted_digest.clone();
            if is_valid_sha256(&digest) {
                return Some(ChainEvidence {
                    event_id: event.id.clone(),
                    source_id: snapshot.source_id.clone(),
                    snapshot_id: snapshot.id.clone(),
                    digest,
                });
            }
        }
    }
    None
}

/// True when `digest` is a well-formed SHA-256 hex digest (64 lowercase hex
/// chars). `"unknown"`, empty, or malformed digests are never accepted.
fn is_valid_sha256(digest: &str) -> bool {
    digest.len() == 64 && digest.chars().all(|c| c.is_ascii_hexdigit())
}

/// Deterministically finds similar-but-not-exact identifier pairs within a
/// matter and returns them as reconciliation-review candidates.
///
/// Two identifiers are considered similar candidates when their normalized
/// forms share a substantial common prefix but are not identical. They are
/// never auto-linked; they are queued for human review.
fn similar_identifier_candidates(
    matter_id: &str,
    identifiers: &[ProcurementIdentifier],
) -> Vec<ReconciliationItem> {
    let normalized: Vec<(&ProcurementIdentifier, &str)> = identifiers
        .iter()
        .filter_map(|i| i.normalized.as_deref().map(|n| (i, n)))
        .collect();

    let mut items = Vec::new();
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for (i, (left, left_norm)) in normalized.iter().enumerate() {
        for (right, right_norm) in normalized.iter().skip(i + 1) {
            if left_norm == right_norm {
                continue; // exact match, not a candidate
            }
            if similar_norm(left_norm, right_norm) {
                let key = if left.raw <= right.raw {
                    format!("{}\0{}", left.raw, right.raw)
                } else {
                    format!("{}\0{}", right.raw, left.raw)
                };
                if seen.insert(key) {
                    items.push(candidate_identifier_item(matter_id, &left.raw, &right.raw));
                }
            }
        }
    }
    items
}

/// A conservative, deterministic "similar but not identical" test on two
/// normalized identifiers. Flags pairs that are plausibly ambiguous — one is a
/// short truncation/abbreviation of the other, or they share a long prefix and
/// diverge only in a short suffix — and never returns true for identical forms.
fn similar_norm(left: &str, right: &str) -> bool {
    if left == right || left.is_empty() || right.is_empty() {
        return false;
    }
    let shared = common_prefix_len(left, right);
    if shared < 3 {
        return false;
    }
    let left_tail = &left[shared..];
    let right_tail = &right[shared..];
    // One identifier is a short truncation of the other, or both share a long
    // prefix and diverge only in a short suffix. Bounded tails keep this from
    // flagging unrelated identifiers that merely share a prefix.
    (left_tail.len() <= 3 && right_tail.len() <= 3)
        || (shared >= 5 && (left_tail.len() <= 3 || right_tail.len() <= 3))
}

fn common_prefix_len(left: &str, right: &str) -> usize {
    left.chars()
        .zip(right.chars())
        .take_while(|(a, b)| a == b)
        .count()
}

/// Verifies the exact-identifier rule between two identifiers, returning the
/// shared normalized key when they match. Used by tests and callers that want
/// the link rule stated explicitly.
pub fn linked_by_exact_identifier(
    left: &ProcurementIdentifier,
    right: &ProcurementIdentifier,
) -> Option<String> {
    match exact_identifier_match(left, right) {
        Ok(true) => left.normalized.clone(),
        _ => None,
    }
}

/// Renders the chain view deterministically for the operator CLI.
pub fn render(chain: &ChainView) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Chain for {} ({})",
        chain.matter.id, chain.matter.title
    );
    out.push('\n');

    for observation in &chain.stages {
        let _ = writeln!(out, "[{}]", observation.stage.label());
        if observation.events.is_empty() {
            let _ = writeln!(out, "    {NOT_OBSERVED_PHRASING}");
        } else {
            for event in &observation.events {
                let _ = writeln!(
                    out,
                    "    observed: {} — {} (source {})",
                    event.date.as_deref().unwrap_or("date unknown"),
                    event.summary,
                    event.source_id
                );
            }
        }
    }

    out.push('\n');
    if chain.links.is_empty() {
        out.push_str("Links:\n    none established by exact, digest-bound evidence.\n");
    } else {
        out.push_str("Links:\n");
        for link in &chain.links {
            let _ = writeln!(
                out,
                "    {} -> {} [{}]",
                link.from.label(),
                link.to.label(),
                link.kind.label()
            );
            if let Some(normalized) = &link.normalized_identifier {
                let _ = writeln!(out, "        normalized identifier: {normalized}");
            }
            let _ = writeln!(out, "        citation: {}", link.citation);
            let _ = writeln!(
                out,
                "        from evidence: event {}, snapshot {}, digest {}",
                link.from_evidence.event_id,
                link.from_evidence.snapshot_id,
                link.from_evidence.digest
            );
            let _ = writeln!(
                out,
                "        to evidence:   event {}, snapshot {}, digest {}",
                link.to_evidence.event_id, link.to_evidence.snapshot_id, link.to_evidence.digest
            );
        }
    }

    out.push('\n');
    if chain.evidence_gaps.is_empty() {
        out.push_str("Evidence gaps:\n    none.\n");
    } else {
        out.push_str("Evidence gaps (links rejected — missing digest-bound evidence):\n");
        for gap in &chain.evidence_gaps {
            let _ = writeln!(
                out,
                "    {} — from event {} (has evidence: {}), to event {} (has evidence: {})",
                gap.normalized_identifier,
                gap.from_event_id,
                gap.from_has_evidence,
                gap.to_event_id,
                gap.to_has_evidence
            );
        }
    }

    out.push('\n');
    if chain.review_suggestions.is_empty() {
        out.push_str("Review suggestions:\n    none.\n");
    } else {
        out.push_str("Review suggestions (in-memory, not persisted reconciliation items):\n");
        for item in &chain.review_suggestions {
            let _ = writeln!(out, "    [{}] {}", item.kind.label(), item.summary);
        }
    }

    out.push('\n');
    out.push_str("Unobserved stages (not proof of absence):\n");
    for stage in &chain.not_observed {
        let _ = writeln!(out, "    {} — {NOT_OBSERVED_PHRASING}", stage.label());
    }
    if chain.not_observed.is_empty() {
        out.push_str("    none — every stage is observed.\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{Acquisition, record_snapshot};
    use pnull_core::{
        IdentifierKind, OrganizationRole, ProcurementEventKind, ProcurementMatter,
        ProcurementOrganization, SourceSnapshot,
    };

    fn seed(store: &Store, matter_id: &str) -> Vec<ProcurementIdentifier> {
        store
            .insert_procurement_matter(&ProcurementMatter {
                id: matter_id.to_owned(),
                jurisdiction: "Colorado Springs".to_owned(),
                title: "Chain test".to_owned(),
                review_state: "draft".to_owned(),
                publication_state: "unpublished".to_owned(),
            })
            .expect("matter");

        let mut ids = Vec::new();
        for (raw, kind) in [
            ("R23-T119KK", IdentifierKind::SolicitationNumber),
            ("R23-T119K", IdentifierKind::SolicitationNumber),
        ] {
            let (normalized, rule) = match pnull_core::normalize_identifier(raw) {
                Some((k, r)) => (Some(k), Some(r.to_owned())),
                None => (None, None),
            };
            let id = ProcurementIdentifier {
                id: ProcurementIdentifier::id_for(matter_id, kind, raw),
                matter_id: matter_id.to_owned(),
                kind,
                raw: raw.to_owned(),
                source_id: "src".to_owned(),
                normalized,
                normalization_rule: rule,
                known: false,
            };
            store
                .insert_procurement_identifier(&id)
                .expect("identifier");
            ids.push(id);
        }
        ids
    }

    fn insert_event(
        store: &Store,
        matter_id: &str,
        kind: ProcurementEventKind,
        date: &str,
        summary: &str,
        identifier_id: &str,
        evidence_ids: &[String],
    ) {
        let event = ProcurementEvent {
            id: ProcurementEvent::id_for(matter_id, kind, date, summary),
            matter_id: matter_id.to_owned(),
            kind,
            date: Some(date.to_owned()),
            summary: summary.to_owned(),
            identifier_ids: vec![identifier_id.to_owned()],
            evidence_ids: evidence_ids.to_vec(),
            source_id: "colorado-springs-contract-awards".to_owned(),
        };
        store.insert_procurement_event(&event).expect("event");
    }

    /// Records a snapshot with the given persisted digest for a source and
    /// returns the exact `SourceSnapshot` (what ingestion binds events to).
    fn seed_snapshot(store: &Store, source_id: &str, digest: &str, at: &str) -> SourceSnapshot {
        let acquisition = Acquisition {
            source_id: source_id.to_owned(),
            source_url: format!("https://example.test/{source_id}"),
            retrieved_at: at.to_owned(),
            bytes_digest: digest.to_owned(),
            content_type: Some("text/html".to_owned()),
            etag: None,
            last_modified: None,
            final_url: format!("https://example.test/{source_id}"),
            redirect_history: Vec::new(),
            parser_version: "awards-1.0".to_owned(),
            schema_version: 2,
            authority: pnull_core::SourceAuthority::OfficialInformationalMirror,
            coverage_state: pnull_core::CoverageState::InformationalOnly,
            observations: Vec::new(),
        };
        let (snapshot, _) =
            record_snapshot(store, &acquisition, None, Some(1), &[], &[]).expect("snapshot");
        snapshot
    }

    /// A deterministic, well-formed SHA-256 digest for tests.
    fn digest(seed: &str) -> String {
        pnull_core::sha256_hex(seed.as_bytes())
    }

    fn store_for(matter_id: &str) -> (tempfile::TempDir, Store, Vec<ProcurementIdentifier>) {
        let dir = tempfile::tempdir().expect("temp");
        let store = Store::open(dir.path()).expect("store");
        let ids = seed(&store, matter_id);
        (dir, store, ids)
    }

    #[test]
    fn exact_identifiers_link_across_stages() {
        let (_dir, store, ids) = store_for("m:exact");
        // The same normalized identifier R23T119KK appears at both the
        // solicitation and award stages, and both events are bound to the same
        // exact snapshot digest -> exact link with digest-bound evidence.
        let snapshot = seed_snapshot(
            &store,
            "colorado-springs-contract-awards",
            &digest("exact"),
            "2026-08-17T00:00:00Z",
        );
        let evidence = vec![snapshot.id.clone()];
        insert_event(
            &store,
            "m:exact",
            ProcurementEventKind::SolicitationPublished,
            "2023-01-15",
            "Traffic Signal On-Call solicitation published",
            &ids[0].id,
            &evidence,
        );
        insert_event(
            &store,
            "m:exact",
            ProcurementEventKind::AwardAnnounced,
            "2024-01-01",
            "Traffic Signal On-Call awarded",
            &ids[0].id,
            &evidence,
        );
        let chain = build_chain(&store, "m:exact").expect("chain");
        assert_eq!(chain.links.len(), 1);
        assert_eq!(chain.links[0].from, ChainStage::Solicitation);
        assert_eq!(chain.links[0].to, ChainStage::Award);
        assert_eq!(chain.links[0].kind, LinkKind::ExactIdentifier);
        assert_eq!(
            chain.links[0].normalized_identifier.as_deref(),
            Some("R23T119KK")
        );
        // Both endpoints carry digest-bound evidence from the exact snapshot.
        assert_eq!(chain.links[0].from_evidence.snapshot_id, snapshot.id);
        assert_eq!(chain.links[0].to_evidence.snapshot_id, snapshot.id);
        assert!(is_valid_sha256(&chain.links[0].from_evidence.digest));
        assert!(is_valid_sha256(&chain.links[0].to_evidence.digest));
        assert!(chain.evidence_gaps.is_empty());
        // The award stage is observed; contract and expenditure are not.
        assert!(!chain.not_observed.contains(&ChainStage::Solicitation));
        assert!(!chain.not_observed.contains(&ChainStage::Award));
        assert!(chain.not_observed.contains(&ChainStage::Contract));
        assert!(chain.not_observed.contains(&ChainStage::Expenditure));
    }

    #[test]
    fn link_without_digest_bound_evidence_is_rejected_and_gapped() {
        let (_dir, store, ids) = store_for("m:noevidence");
        // Both events share the exact normalized identifier but are NOT bound
        // to any snapshot. The link must not be created; an evidence gap is
        // rendered instead.
        insert_event(
            &store,
            "m:noevidence",
            ProcurementEventKind::SolicitationPublished,
            "2023-01-15",
            "solicitation",
            &ids[0].id,
            &[],
        );
        insert_event(
            &store,
            "m:noevidence",
            ProcurementEventKind::AwardAnnounced,
            "2024-01-01",
            "award",
            &ids[0].id,
            &[],
        );
        let chain = build_chain(&store, "m:noevidence").expect("chain");
        assert!(
            chain.links.is_empty(),
            "a link must never be accepted without digest-bound evidence"
        );
        assert_eq!(chain.evidence_gaps.len(), 1);
        assert!(!chain.evidence_gaps[0].from_has_evidence);
        assert!(!chain.evidence_gaps[0].to_has_evidence);
        let rendered = render(&chain);
        assert!(rendered.contains("Evidence gaps"));
    }

    #[test]
    fn similar_but_not_exact_identifiers_never_link_and_are_queued() {
        let (_dir, store, ids) = store_for("m:similar");
        // R23-T119KK (solicitation) and R23-T119K (award) are similar but not
        // identical. Even with digest-bound evidence they must not auto-link.
        let snapshot = seed_snapshot(
            &store,
            "colorado-springs-contract-awards",
            &digest("similar"),
            "2026-08-17T00:00:00Z",
        );
        let evidence = vec![snapshot.id.clone()];
        insert_event(
            &store,
            "m:similar",
            ProcurementEventKind::SolicitationPublished,
            "2023-01-15",
            "solicitation",
            &ids[0].id,
            &evidence,
        );
        insert_event(
            &store,
            "m:similar",
            ProcurementEventKind::AwardAnnounced,
            "2024-01-01",
            "award",
            &ids[1].id,
            &evidence,
        );
        let chain = build_chain(&store, "m:similar").expect("chain");
        assert!(
            chain.links.is_empty(),
            "similar identifiers must never auto-link"
        );
        // The ambiguous pair is surfaced as a review suggestion, not a link.
        assert!(
            chain
                .review_suggestions
                .iter()
                .any(|i| i.summary.contains("R23-T119K"))
        );
    }

    #[test]
    fn ambiguous_candidates_enter_reconciliation() {
        let (_dir, store, ids) = store_for("m:ambig");
        insert_event(
            &store,
            "m:ambig",
            ProcurementEventKind::AwardAnnounced,
            "2024-01-01",
            "award a",
            &ids[0].id,
            &[],
        );
        insert_event(
            &store,
            "m:ambig",
            ProcurementEventKind::AwardAnnounced,
            "2024-01-02",
            "award b",
            &ids[1].id,
            &[],
        );
        let chain = build_chain(&store, "m:ambig").expect("chain");
        assert!(chain.links.is_empty());
        assert!(!chain.review_suggestions.is_empty());
    }

    #[test]
    fn missing_stages_render_as_not_observed() {
        let (_dir, store, ids) = store_for("m:gaps");
        insert_event(
            &store,
            "m:gaps",
            ProcurementEventKind::SolicitationPublished,
            "2023-01-15",
            "solicitation only",
            &ids[0].id,
            &[],
        );
        let chain = build_chain(&store, "m:gaps").expect("chain");
        let rendered = render(&chain);
        assert!(rendered.contains("Not observed in the checked sources."));
        assert!(rendered.contains("[expenditure]"));
        // The rendered output never claims absence is proof.
        assert!(!rendered.contains("no record exists"));
    }

    #[test]
    fn each_accepted_link_traces_to_digest_bound_evidence() {
        let (_dir, store, ids) = store_for("m:digest");
        let snapshot = seed_snapshot(
            &store,
            "colorado-springs-contract-awards",
            &digest("digest"),
            "2026-08-17T00:00:00Z",
        );
        let evidence = vec![snapshot.id.clone()];
        insert_event(
            &store,
            "m:digest",
            ProcurementEventKind::SolicitationPublished,
            "2023-01-15",
            "solicitation",
            &ids[0].id,
            &evidence,
        );
        insert_event(
            &store,
            "m:digest",
            ProcurementEventKind::AwardAnnounced,
            "2024-01-01",
            "award",
            &ids[0].id,
            &evidence,
        );
        let chain = build_chain(&store, "m:digest").expect("chain");
        assert_eq!(chain.links.len(), 1);
        let link = &chain.links[0];
        assert!(!link.citation.is_empty());
        // Digest-bound evidence must be a valid SHA-256, never "unknown".
        assert!(is_valid_sha256(&link.from_evidence.digest));
        assert!(is_valid_sha256(&link.to_evidence.digest));
        assert_ne!(link.from_evidence.digest, "unknown");
        assert_eq!(link.from_evidence.snapshot_id, snapshot.id);
        // The citation names both linked events.
        assert!(link.citation.contains("solicitation"));
        assert!(link.citation.contains("award"));
    }

    #[test]
    fn ingested_data_is_reachable_through_its_matter() {
        let (_dir, store, ids) = store_for("m:reachable");
        insert_event(
            &store,
            "m:reachable",
            ProcurementEventKind::AwardAnnounced,
            "2024-01-01",
            "award",
            &ids[0].id,
            &[],
        );
        // The identifier must resolve through the real matter.
        let matter = store.procurement_matter("m:reachable").expect("matter");
        assert_eq!(matter.id, "m:reachable");
        let matter_ids = store.procurement_identifiers("m:reachable").expect("ids");
        assert!(matter_ids.iter().any(|i| i.raw == "R23-T119KK"));
    }

    #[test]
    fn identical_runs_produce_identical_chain_output() {
        let (dir_a, store_a, _) = store_for("m:determ");
        let (dir_b, store_b, _) = store_for("m:determ");
        let _ = &dir_a;
        let _ = &dir_b;
        // Build identical data in both stores, including the exact snapshot
        // that ingestion binds events to.
        for (store, _) in [(&store_a, 0usize), (&store_b, 0usize)] {
            let snapshot = seed_snapshot(
                store,
                "colorado-springs-contract-awards",
                &digest("identical"),
                "2026-08-17T00:00:00Z",
            );
            let evidence = vec![snapshot.id.clone()];
            let ids = store.procurement_identifiers("m:determ").expect("ids");
            let sol = ids.iter().find(|i| i.raw == "R23-T119KK").expect("sol");
            insert_event(
                store,
                "m:determ",
                ProcurementEventKind::SolicitationPublished,
                "2023-01-15",
                "solicitation",
                &sol.id,
                &evidence,
            );
            insert_event(
                store,
                "m:determ",
                ProcurementEventKind::AwardAnnounced,
                "2024-01-01",
                "award",
                &sol.id,
                &evidence,
            );
        }
        let chain_a = build_chain(&store_a, "m:determ").expect("chain a");
        let chain_b = build_chain(&store_b, "m:determ").expect("chain b");
        assert_eq!(render(&chain_a), render(&chain_b));
    }

    #[test]
    fn stage_mapping_is_complete_and_ordered() {
        let labels: Vec<&str> = ChainStage::ALL.iter().map(|s| s.label()).collect();
        assert_eq!(
            labels,
            vec![
                "solicitation",
                "amendment",
                "award",
                "contract",
                "expenditure"
            ]
        );
        assert_eq!(
            ChainStage::from_event_kind(ProcurementEventKind::ContractExecuted),
            Some(ChainStage::Contract)
        );
        assert_eq!(
            ChainStage::from_event_kind(ProcurementEventKind::ExpenditureReported),
            Some(ChainStage::Expenditure)
        );
        assert_eq!(
            ChainStage::from_event_kind(ProcurementEventKind::RecordCorrected),
            None
        );
    }

    #[test]
    fn organizations_do_not_affect_chain_linking() {
        let (_dir, store, ids) = store_for("m:orgs");
        // An unrelated organization is stored but does not create a link.
        let org = ProcurementOrganization {
            id: ProcurementOrganization::id_for(
                "m:orgs",
                OrganizationRole::AwardedContractor,
                "Crafco",
            ),
            matter_id: "m:orgs".to_owned(),
            role: OrganizationRole::AwardedContractor,
            raw_name: "Crafco".to_owned(),
            source_id: "src".to_owned(),
            normalized_alias: None,
            alias_reviewed: false,
        };
        store.insert_procurement_organization(&org).expect("org");
        let chain = build_chain(&store, "m:orgs").expect("chain");
        assert!(chain.links.is_empty());
        assert!(ids.len() >= 2);
    }

    #[test]
    fn newer_snapshot_does_not_change_old_link_evidence() {
        let (_dir, store, ids) = store_for("m:regress");
        // Ingestion records snapshot A; both events are bound to it.
        let snapshot_a = seed_snapshot(
            &store,
            "colorado-springs-contract-awards",
            &digest("regress-a"),
            "2026-08-17T00:00:00Z",
        );
        let evidence_a = vec![snapshot_a.id.clone()];
        insert_event(
            &store,
            "m:regress",
            ProcurementEventKind::SolicitationPublished,
            "2023-01-15",
            "solicitation",
            &ids[0].id,
            &evidence_a,
        );
        insert_event(
            &store,
            "m:regress",
            ProcurementEventKind::AwardAnnounced,
            "2024-01-01",
            "award",
            &ids[0].id,
            &evidence_a,
        );
        let chain_before = build_chain(&store, "m:regress").expect("chain before");
        assert_eq!(chain_before.links.len(), 1);
        let old_digest = chain_before.links[0].from_evidence.digest.clone();
        let old_snapshot_id = chain_before.links[0].from_evidence.snapshot_id.clone();
        assert_eq!(old_digest, snapshot_a.persisted_digest);

        // A newer snapshot for the same source is ingested later.
        let snapshot_b = seed_snapshot(
            &store,
            "colorado-springs-contract-awards",
            &digest("regress-b"),
            "2026-09-01T00:00:00Z",
        );
        assert_ne!(snapshot_b.persisted_digest, snapshot_a.persisted_digest);

        // Rebuilding the chain must still bind the link to snapshot A, not to
        // whichever snapshot is newest today.
        let chain_after = build_chain(&store, "m:regress").expect("chain after");
        assert_eq!(chain_after.links.len(), 1);
        assert_eq!(
            chain_after.links[0].from_evidence.digest, old_digest,
            "a newer snapshot must not retroactively rebind an existing link"
        );
        assert_eq!(
            chain_after.links[0].from_evidence.snapshot_id,
            old_snapshot_id
        );
        assert_eq!(
            chain_after.links[0].to_evidence.digest,
            snapshot_a.persisted_digest
        );
        // The newer snapshot never appears as accepted link evidence.
        for link in &chain_after.links {
            assert_ne!(link.from_evidence.digest, snapshot_b.persisted_digest);
            assert_ne!(link.to_evidence.digest, snapshot_b.persisted_digest);
        }
    }
}
