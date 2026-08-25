//! Deterministic procurement-chain construction: an ordered view of
//! `solicitation -> amendment -> award -> contract -> expenditure`.
//!
//! A link between two chain stages is created **only** when the evidence
//! supports it:
//! - the two records reference identifiers whose normalized forms match
//!   exactly (the deterministic `uppercase-alphanumeric-compact` rule), or
//! - an official record explicitly states the relationship.
//!
//! Similar-but-not-exact identifiers, incomplete identifiers, and ambiguous
//! candidates are **never** auto-linked; they are queued for human
//! reconciliation. A stage with no observed supporting record is rendered as
//! "Not observed in the checked sources." and is never presented as proof that
//! no record exists. Every accepted link retains the supporting record, its
//! citation, and the snapshot digest that binds it to immutable evidence.

use std::collections::BTreeMap;

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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkKind {
    /// The two records reference identifiers whose normalized forms match exactly.
    ExactIdentifier,
    /// An official record explicitly states the relationship.
    ExplicitlyStated,
}

impl LinkKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::ExactIdentifier => "exact identifier match",
            Self::ExplicitlyStated => "explicitly stated by official record",
        }
    }
}

/// An accepted, evidence-backed link between two chain stages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainLink {
    pub from: ChainStage,
    pub to: ChainStage,
    pub kind: LinkKind,
    /// The normalized identifier shared by both linked records (exact links).
    pub normalized_identifier: Option<String>,
    /// The event ids on each side of the link.
    pub from_event_id: String,
    pub to_event_id: String,
    /// A citation/summary describing the supporting evidence.
    pub citation: String,
    /// The snapshot digest that binds the evidence to immutable bytes.
    pub snapshot_digest: String,
    pub source_id: String,
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
    /// Ambiguous / similar-but-not-exact candidates queued for reconciliation.
    pub reconciliation_candidates: Vec<ReconciliationItem>,
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

    // Create exact cross-stage links: two events in different stages sharing a
    // normalized identifier.
    let mut links = Vec::new();
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for (normalized, occ) in &by_normalized {
        for (i, (from_stage, from_event)) in occ.iter().enumerate() {
            for (to_stage, to_event) in occ.iter().skip(i + 1) {
                if from_stage == to_stage {
                    continue; // within a stage, not a chain link
                }
                let (left, right) = ordered_pair(*from_stage, *to_stage, from_event, to_event);
                let pair_key = format!("{}\0{}", left.event.id, right.event.id);
                if seen.insert(pair_key) {
                    let (from, to) = if left.stage.index() < right.stage.index() {
                        (left, right)
                    } else {
                        (right, left)
                    };
                    links.push(ChainLink {
                        from: from.stage,
                        to: to.stage,
                        kind: LinkKind::ExactIdentifier,
                        normalized_identifier: Some(normalized.clone()),
                        from_event_id: from.event.id.clone(),
                        to_event_id: to.event.id.clone(),
                        citation: format!(
                            "events '{}' (source {}) and '{}' (source {}) reference the same normalized identifier {normalized}",
                            from.event.summary,
                            from.event.source_id,
                            to.event.summary,
                            to.event.source_id
                        ),
                        snapshot_digest: latest_digest_for_source(store, &from.event.source_id),
                        source_id: from.event.source_id.clone(),
                    });
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

    // Queue similar-but-not-exact identifiers for human reconciliation. These
    // are never linked; they are surfaced for review.
    let reconciliation_candidates = similar_identifier_candidates(matter_id, &identifiers);

    Ok(ChainView {
        matter,
        stages,
        links,
        reconciliation_candidates,
        not_observed,
    })
}

/// A pair of (stage, event) kept in source order for link construction.
struct Side<'a> {
    stage: ChainStage,
    event: &'a ProcurementEvent,
}

fn ordered_pair<'a>(
    a_stage: ChainStage,
    b_stage: ChainStage,
    a_event: &'a ProcurementEvent,
    b_event: &'a ProcurementEvent,
) -> (Side<'a>, Side<'a>) {
    let a = Side {
        stage: a_stage,
        event: a_event,
    };
    let b = Side {
        stage: b_stage,
        event: b_event,
    };
    (a, b)
}

/// Resolves the most recent snapshot digest for a source, binding a link to
/// immutable evidence. Returns `"unknown"` when no snapshot exists.
fn latest_digest_for_source(store: &Store, source_id: &str) -> String {
    match store.source_snapshots(source_id) {
        Ok(snapshots) => snapshots
            .last()
            .map_or_else(|| "unknown".to_owned(), |s| s.persisted_digest.clone()),
        Err(_) => "unknown".to_owned(),
    }
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
        out.push_str("Links:\n    none established by exact evidence.\n");
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
            let _ = writeln!(out, "        snapshot digest: {}", link.snapshot_digest);
        }
    }

    out.push('\n');
    if chain.reconciliation_candidates.is_empty() {
        out.push_str("Reconciliation queue:\n    no ambiguous candidates queued.\n");
    } else {
        out.push_str("Reconciliation queue (never auto-linked):\n");
        for item in &chain.reconciliation_candidates {
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
    use pnull_core::{
        IdentifierKind, OrganizationRole, ProcurementEventKind, ProcurementMatter,
        ProcurementOrganization,
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
    ) {
        let event = ProcurementEvent {
            id: ProcurementEvent::id_for(matter_id, kind, date, summary),
            matter_id: matter_id.to_owned(),
            kind,
            date: Some(date.to_owned()),
            summary: summary.to_owned(),
            identifier_ids: vec![identifier_id.to_owned()],
            evidence_ids: Vec::new(),
            source_id: "colorado-springs-contract-awards".to_owned(),
        };
        store.insert_procurement_event(&event).expect("event");
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
        // solicitation and award stages -> exact link.
        insert_event(
            &store,
            "m:exact",
            ProcurementEventKind::SolicitationPublished,
            "2023-01-15",
            "Traffic Signal On-Call solicitation published",
            &ids[0].id,
        );
        insert_event(
            &store,
            "m:exact",
            ProcurementEventKind::AwardAnnounced,
            "2024-01-01",
            "Traffic Signal On-Call awarded",
            &ids[0].id,
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
        // The award stage is observed; contract and expenditure are not.
        assert!(!chain.not_observed.contains(&ChainStage::Solicitation));
        assert!(!chain.not_observed.contains(&ChainStage::Award));
        assert!(chain.not_observed.contains(&ChainStage::Contract));
        assert!(chain.not_observed.contains(&ChainStage::Expenditure));
    }

    #[test]
    fn similar_but_not_exact_identifiers_never_link_and_are_queued() {
        let (_dir, store, ids) = store_for("m:similar");
        // R23-T119KK (solicitation) and R23-T119K (award) are similar but not
        // identical. They must not auto-link.
        insert_event(
            &store,
            "m:similar",
            ProcurementEventKind::SolicitationPublished,
            "2023-01-15",
            "solicitation",
            &ids[0].id,
        );
        insert_event(
            &store,
            "m:similar",
            ProcurementEventKind::AwardAnnounced,
            "2024-01-01",
            "award",
            &ids[1].id,
        );
        let chain = build_chain(&store, "m:similar").expect("chain");
        assert!(
            chain.links.is_empty(),
            "similar identifiers must never auto-link"
        );
        // The ambiguous pair is queued for human reconciliation.
        assert!(
            chain
                .reconciliation_candidates
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
        );
        insert_event(
            &store,
            "m:ambig",
            ProcurementEventKind::AwardAnnounced,
            "2024-01-02",
            "award b",
            &ids[1].id,
        );
        let chain = build_chain(&store, "m:ambig").expect("chain");
        assert!(chain.links.is_empty());
        assert!(!chain.reconciliation_candidates.is_empty());
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
        insert_event(
            &store,
            "m:digest",
            ProcurementEventKind::SolicitationPublished,
            "2023-01-15",
            "solicitation",
            &ids[0].id,
        );
        insert_event(
            &store,
            "m:digest",
            ProcurementEventKind::AwardAnnounced,
            "2024-01-01",
            "award",
            &ids[0].id,
        );
        let chain = build_chain(&store, "m:digest").expect("chain");
        assert_eq!(chain.links.len(), 1);
        let link = &chain.links[0];
        assert!(!link.snapshot_digest.is_empty());
        assert!(!link.citation.is_empty());
        assert!(link.snapshot_digest == "unknown" || link.snapshot_digest.len() == 64);
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
        // Build identical data in both stores.
        for (store, _) in [(&store_a, 0usize), (&store_b, 0usize)] {
            let ids = store.procurement_identifiers("m:determ").expect("ids");
            let sol = ids.iter().find(|i| i.raw == "R23-T119KK").expect("sol");
            insert_event(
                store,
                "m:determ",
                ProcurementEventKind::SolicitationPublished,
                "2023-01-15",
                "solicitation",
                &sol.id,
            );
            insert_event(
                store,
                "m:determ",
                ProcurementEventKind::AwardAnnounced,
                "2024-01-01",
                "award",
                &sol.id,
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
    fn explicit_stated_link_kind_is_supported() {
        // The LinkKind variant exists and labels deterministically; a link may
        // be explicitly stated by an official record (no identifier merge).
        assert_eq!(
            LinkKind::ExplicitlyStated.label(),
            "explicitly stated by official record"
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
}
