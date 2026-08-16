//! Transparent deterministic surveillance detection and document comparison.

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::Path;

use pnull_core::{
    Action, ActionKind, Alert, Citation, DiffChange, DocumentRole, EvidenceDiff, EvidenceRecord,
    Finding, FindingState, Locator, SourceType, Subject, SubjectKind, stable_id,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RuleSet {
    pub version: u32,
    pub rules: Vec<Rule>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Rule {
    pub id: String,
    pub label: String,
    pub category: String,
    pub terms: Vec<String>,
    #[serde(default)]
    pub false_positive_phrases: Vec<String>,
    pub rationale: String,
}

#[derive(Debug, Error)]
pub enum DetectError {
    #[error("could not read rule file: {0}")]
    Io(#[from] std::io::Error),
    #[error("rule file is invalid: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("rule set is invalid: {0}")]
    InvalidRules(String),
}

pub fn load_rules(path: impl AsRef<Path>) -> Result<RuleSet, DetectError> {
    let rules: RuleSet = serde_yaml::from_slice(&fs::read(path)?)?;
    validate_rules(&rules)?;
    Ok(rules)
}

fn validate_rules(rules: &RuleSet) -> Result<(), DetectError> {
    if rules.version == 0 || rules.rules.is_empty() {
        return Err(DetectError::InvalidRules(
            "version must be positive and at least one rule is required".to_owned(),
        ));
    }
    let mut ids = HashSet::new();
    for rule in &rules.rules {
        if rule.id.trim().is_empty() || rule.terms.is_empty() {
            return Err(DetectError::InvalidRules(
                "every rule requires an identifier and terms".to_owned(),
            ));
        }
        if !ids.insert(&rule.id) {
            return Err(DetectError::InvalidRules(format!(
                "duplicate rule identifier: {}",
                rule.id
            )));
        }
    }
    Ok(())
}

pub fn scan(record: &EvidenceRecord, text: &str, rules: &RuleSet) -> Option<Finding> {
    let mut matched = BTreeSet::new();
    let mut citations = Vec::new();
    for rule in &rules.rules {
        for term in &rule.terms {
            let line_index = find_term_lines(text, term).into_iter().find(|index| {
                let line = text.lines().nth(*index).unwrap_or_default().to_lowercase();
                !rule
                    .false_positive_phrases
                    .iter()
                    .any(|phrase| line.contains(&phrase.to_lowercase()))
            });
            if let Some(line_index) = line_index {
                matched.insert(rule.id.clone());
                citations.push(citation(record, text, line_index));
                break;
            }
        }
    }
    if matched.is_empty() || citations.is_empty() {
        return None;
    }
    citations.sort_by_key(|item| item.locator.start);
    citations.dedup_by(|left, right| {
        left.evidence_id == right.evidence_id && left.locator.start == right.locator.start
    });
    let (state, reason, state_citation) = classify_near(record, text, &citations);
    if let Some(item) = state_citation {
        citations.push(item);
    }
    let matched_rule_ids: Vec<String> = matched.into_iter().collect();
    let digest = rules_digest(rules);
    let id = stable_id(
        "finding",
        &[
            &record.id,
            state.label(),
            &matched_rule_ids.join(","),
            &rules.version.to_string(),
            &digest,
        ],
    );
    Some(Finding {
        id,
        evidence_id: record.id.clone(),
        jurisdiction: record.jurisdiction.clone(),
        state,
        classification_reason: reason,
        rules_version: rules.version,
        rules_digest: digest,
        matched_rule_ids,
        citations,
    })
}

fn term_regex(term: &str) -> Option<Regex> {
    let escaped = regex::escape(term);
    let starts_word = term.chars().next().is_some_and(char::is_alphanumeric);
    let ends_word = term.chars().last().is_some_and(char::is_alphanumeric);
    Regex::new(&format!(
        "(?i){}{}{}",
        if starts_word { r"\b" } else { "" },
        escaped,
        if ends_word { r"\b" } else { "" }
    ))
    .ok()
}

fn find_term_lines(text: &str, term: &str) -> Vec<usize> {
    term_regex(term).map_or_else(Vec::new, |regex| {
        text.lines()
            .enumerate()
            .filter_map(|(index, line)| regex.is_match(line).then_some(index))
            .collect()
    })
}

fn find_term_line(text: &str, term: &str) -> Option<usize> {
    find_term_lines(text, term).into_iter().next()
}

pub fn classify_document(
    record: &EvidenceRecord,
    text: &str,
) -> (FindingState, String, Option<Citation>) {
    classify_with_filter(record, text, |_| true)
}

fn classify_near(
    record: &EvidenceRecord,
    text: &str,
    term_citations: &[Citation],
) -> (FindingState, String, Option<Citation>) {
    classify_with_filter(record, text, |index| {
        text.lines()
            .nth(index)
            .is_some_and(|line| line.starts_with("Action:"))
            || term_citations.iter().any(|item| {
                usize::try_from(item.locator.start)
                    .ok()
                    .is_some_and(|line| line.abs_diff(index + 1) <= 3)
            })
    })
}

fn classify_with_filter<F: Fn(usize) -> bool>(
    record: &EvidenceRecord,
    text: &str,
    accepts: F,
) -> (FindingState, String, Option<Citation>) {
    const PATTERNS: &[(FindingState, &[&str], &str)] = &[
        (
            FindingState::ContractExecuted,
            &["contract executed", "signed a", "entered into a contract"],
            "The source explicitly describes an executed or signed contract.",
        ),
        (
            FindingState::RenewalOrExpansion,
            &[
                "renewal",
                "renewed",
                "expansion",
                "expanded",
                "additional technology",
            ],
            "The source explicitly describes a renewal, expansion, or additional system scope.",
        ),
        (
            FindingState::Approved,
            &["finally passed", "approved", "adopted", "authorized"],
            "The source records an approval, adoption, authorization, or final passage.",
        ),
        (
            FindingState::Rejected,
            &["rejected", "denied", "failed by a vote"],
            "The source records rejection, denial, or a failed vote.",
        ),
        (
            FindingState::PublicHearingScheduled,
            &["public hearing scheduled", "public hearing on"],
            "The source explicitly schedules a public hearing.",
        ),
        (
            FindingState::VoteScheduled,
            &["vote scheduled", "will vote", "second reading"],
            "The source explicitly schedules a vote or later reading.",
        ),
        (
            FindingState::PolicyChange,
            &["policy change", "policy amended", "policy revised"],
            "The source explicitly describes a policy change.",
        ),
        (
            FindingState::DeploymentReported,
            &["deployed", "in operation", "currently using", "implemented"],
            "The source explicitly reports deployment or current operation.",
        ),
        (
            FindingState::Proposal,
            &[
                "proposal",
                "proposed",
                "seeks to",
                "request for information",
                "referred",
            ],
            "The source describes a proposal, request, or referral, not a completed purchase.",
        ),
    ];
    let mut candidates = Vec::new();
    for (state, phrases, reason) in PATTERNS {
        for phrase in *phrases {
            for index in find_term_lines(text, phrase) {
                let line = text.lines().nth(index).unwrap_or_default();
                if accepts(index) && !is_negated_or_conditional(line, phrase) {
                    candidates.push((*state, *reason, index));
                }
            }
        }
    }
    candidates.sort_by_key(|candidate| candidate.2);
    candidates.dedup_by_key(|candidate| (candidate.0, candidate.2));
    let distinct: BTreeSet<String> = candidates
        .iter()
        .map(|candidate| candidate.0.label().to_owned())
        .collect();
    if distinct.len() > 1 {
        let index = candidates.last().map_or(0, |candidate| candidate.2);
        return (
            FindingState::Unknown,
            "The source contains multiple potentially conflicting state phrases; no stronger state is asserted without human review.".to_owned(),
            Some(citation(record, text, index)),
        );
    }
    if let Some((state, reason, index)) = candidates.last() {
        return (
            *state,
            (*reason).to_owned(),
            Some(citation(record, text, *index)),
        );
    }
    (
        FindingState::MentionDetected,
        "A taxonomy term appears, but the source span does not establish proposal, approval, purchase, deployment, or another stronger state.".to_owned(),
        None,
    )
}

fn is_negated_or_conditional(line: &str, phrase: &str) -> bool {
    let line = line.to_lowercase();
    let phrase = phrase.to_lowercase();
    [
        format!("not {phrase}"),
        format!("no {phrase}"),
        format!("never {phrase}"),
        format!("if {phrase}"),
        format!("unless {phrase}"),
    ]
    .iter()
    .any(|pattern| line.contains(pattern))
        || (phrase == "approved" && line.contains("subject to approval"))
}

fn citation(record: &EvidenceRecord, text: &str, zero_based_line: usize) -> Citation {
    let line_number = u32::try_from(zero_based_line + 1).unwrap_or(u32::MAX);
    Citation {
        evidence_id: record.id.clone(),
        source_url: record.source_url.clone(),
        locator: Locator {
            kind: "line".to_owned(),
            start: line_number,
            end: line_number,
            label: format!("line {line_number}"),
        },
        quote: text
            .lines()
            .nth(zero_based_line)
            .unwrap_or_default()
            .to_owned(),
    }
}

pub fn compare(
    old_record: &EvidenceRecord,
    old_text: &str,
    new_record: &EvidenceRecord,
    new_text: &str,
    rules: &RuleSet,
) -> EvidenceDiff {
    let mut changes = compare_standard_changes(old_record, old_text, new_record, new_text, rules);
    if old_text.to_lowercase().contains("amendment")
        != new_text.to_lowercase().contains("amendment")
    {
        changes.push(DiffChange {
            kind: "new_contract_or_amendment".to_owned(),
            summary: "The newer document changes whether an amendment is described.".to_owned(),
            before: find_topic_citation(old_record, old_text, &["amendment"]),
            after: find_topic_citation(new_record, new_text, &["amendment"]),
        });
    }
    for topic in ["privacy", "retention", "data sharing", "shall not share"] {
        if let Some(old_line) = find_topic_line(old_text, &[topic])
            && find_topic_line(new_text, &[topic]).is_none()
        {
            changes.push(DiffChange {
                kind: "removed_relevant_language".to_owned(),
                summary: format!("The newer document removes language containing “{topic}”."),
                before: Some(citation(old_record, old_text, old_line)),
                after: None,
            });
        }
    }
    let old_matches = matched_rule_ids(old_text, rules);
    let new_matches = matched_rule_ids(new_text, rules);
    if new_matches.difference(&old_matches).next().is_some() {
        changes.push(DiffChange {
            kind: "new_surveillance_related_agenda_item".to_owned(),
            summary: "The newer document introduces a surveillance taxonomy term not present in the earlier version.".to_owned(),
            before: None,
            after: scan(new_record, new_text, rules).and_then(|finding| finding.citations.first().cloned()),
        });
    }
    changes.sort_by(|left, right| left.kind.cmp(&right.kind));
    changes.dedup_by(|left, right| left.kind == right.kind && left.summary == right.summary);
    let unified_text = meaningful_diff(&changes);
    EvidenceDiff {
        old_evidence_id: old_record.id.clone(),
        new_evidence_id: new_record.id.clone(),
        old_source_url: old_record.source_url.clone(),
        new_source_url: new_record.source_url.clone(),
        changes,
        unified_text,
    }
}

fn compare_standard_changes(
    old_record: &EvidenceRecord,
    old_text: &str,
    new_record: &EvidenceRecord,
    new_text: &str,
    rules: &RuleSet,
) -> Vec<DiffChange> {
    let mut changes = Vec::new();
    for (kind, prefix) in [
        ("changed_vote_date", "Meeting date:"),
        ("changed_procurement_state", "Action:"),
    ] {
        field_change(
            &mut changes,
            kind,
            prefix,
            old_record,
            old_text,
            new_record,
            new_text,
        );
    }
    compare_topic_line(
        &mut changes,
        "changed_vote_date",
        &["finally passed:"],
        old_record,
        old_text,
        new_record,
        new_text,
    );
    for (kind, pattern) in [
        ("changed_price", r"(?i)\$[0-9][0-9,]*(?:\.[0-9]{2})?"),
        (
            "changed_contract_duration",
            r"(?i)\b\d+[- ](?:year|month)s?\b|\b\d+\s+(?:years|months)\b",
        ),
    ] {
        compare_pattern(
            &mut changes,
            kind,
            pattern,
            old_record,
            old_text,
            new_record,
            new_text,
        );
    }
    for (kind, topics) in [
        (
            "changed_retention_provision",
            &["retention", "retain", "delete after"][..],
        ),
        (
            "changed_data_sharing_provision",
            &["data sharing", "share data", "third-party access"][..],
        ),
        (
            "changed_scope_or_quantity",
            &["camera", "drone", "license", "unit", "quantity"][..],
        ),
    ] {
        compare_topic_line(
            &mut changes,
            kind,
            topics,
            old_record,
            old_text,
            new_record,
            new_text,
        );
    }
    let vendor_terms: Vec<&str> = rules
        .rules
        .iter()
        .filter(|rule| rule.category == "vendor")
        .flat_map(|rule| rule.terms.iter().map(String::as_str))
        .collect();
    compare_topic_line(
        &mut changes,
        "changed_vendor_or_subcontractor",
        &vendor_terms,
        old_record,
        old_text,
        new_record,
        new_text,
    );
    changes
}

fn matched_rule_ids(text: &str, rules: &RuleSet) -> BTreeSet<String> {
    rules
        .rules
        .iter()
        .filter(|rule| {
            rule.terms
                .iter()
                .any(|term| find_term_line(text, term).is_some())
        })
        .map(|rule| rule.id.clone())
        .collect()
}

fn field_change(
    changes: &mut Vec<DiffChange>,
    kind: &str,
    prefix: &str,
    old_record: &EvidenceRecord,
    old_text: &str,
    new_record: &EvidenceRecord,
    new_text: &str,
) {
    let old = old_text.lines().position(|line| line.starts_with(prefix));
    let new = new_text.lines().position(|line| line.starts_with(prefix));
    if old.map(|index| old_text.lines().nth(index)) != new.map(|index| new_text.lines().nth(index))
        && (old.is_some() || new.is_some())
    {
        changes.push(DiffChange {
            kind: kind.to_owned(),
            summary: format!(
                "{} changed from “{}” to “{}”.",
                prefix.trim_end_matches(':'),
                old.and_then(|index| old_text.lines().nth(index))
                    .unwrap_or("not stated"),
                new.and_then(|index| new_text.lines().nth(index))
                    .unwrap_or("not stated")
            ),
            before: old.map(|index| citation(old_record, old_text, index)),
            after: new.map(|index| citation(new_record, new_text, index)),
        });
    }
}

fn compare_pattern(
    changes: &mut Vec<DiffChange>,
    kind: &str,
    pattern: &str,
    old_record: &EvidenceRecord,
    old_text: &str,
    new_record: &EvidenceRecord,
    new_text: &str,
) {
    let Ok(regex) = Regex::new(pattern) else {
        return;
    };
    let old_values: BTreeSet<&str> = regex
        .find_iter(old_text)
        .map(|item| item.as_str())
        .collect();
    let new_values: BTreeSet<&str> = regex
        .find_iter(new_text)
        .map(|item| item.as_str())
        .collect();
    if old_values != new_values && (!old_values.is_empty() || !new_values.is_empty()) {
        changes.push(DiffChange {
            kind: kind.to_owned(),
            summary: format!(
                "Values changed from [{}] to [{}].",
                old_values.into_iter().collect::<Vec<_>>().join(", "),
                new_values.into_iter().collect::<Vec<_>>().join(", ")
            ),
            before: regex
                .find(old_text)
                .map(|item| citation(old_record, old_text, line_index_at(old_text, item.start()))),
            after: regex
                .find(new_text)
                .map(|item| citation(new_record, new_text, line_index_at(new_text, item.start()))),
        });
    }
}

fn compare_topic_line(
    changes: &mut Vec<DiffChange>,
    kind: &str,
    topics: &[&str],
    old_record: &EvidenceRecord,
    old_text: &str,
    new_record: &EvidenceRecord,
    new_text: &str,
) {
    let old = find_topic_line(old_text, topics);
    let new = find_topic_line(new_text, topics);
    let old_line = old.and_then(|index| old_text.lines().nth(index));
    let new_line = new.and_then(|index| new_text.lines().nth(index));
    if old_line != new_line && (old.is_some() && new.is_some()) {
        changes.push(DiffChange {
            kind: kind.to_owned(),
            summary: format!(
                "Relevant language changed from “{}” to “{}”.",
                old_line.unwrap_or_default(),
                new_line.unwrap_or_default()
            ),
            before: old.map(|index| citation(old_record, old_text, index)),
            after: new.map(|index| citation(new_record, new_text, index)),
        });
    }
}

fn find_topic_line(text: &str, topics: &[&str]) -> Option<usize> {
    text.lines().position(|line| {
        let lowercase = line.to_lowercase();
        topics
            .iter()
            .any(|topic| lowercase.contains(&topic.to_lowercase()))
    })
}

fn find_topic_citation(record: &EvidenceRecord, text: &str, topics: &[&str]) -> Option<Citation> {
    find_topic_line(text, topics).map(|index| citation(record, text, index))
}

fn meaningful_diff(changes: &[DiffChange]) -> String {
    let mut lines = vec![
        "--- earlier evidence".to_owned(),
        "+++ newer evidence".to_owned(),
    ];
    for change in changes {
        lines.push(format!("@@ {} @@", change.kind));
        if let Some(before) = &change.before {
            lines.push(format!("- {}: {}", before.locator.label, before.quote));
        }
        if let Some(after) = &change.after {
            lines.push(format!("+ {}: {}", after.locator.label, after.quote));
        }
    }
    lines.join("\n")
}

fn line_index_at(text: &str, byte_index: usize) -> usize {
    text[..byte_index.min(text.len())].matches('\n').count()
}

#[cfg(test)]
fn unified_text(old: &str, new: &str) -> String {
    const MAX_CHANGED_LINES: usize = 200;
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let prefix = old_lines
        .iter()
        .zip(&new_lines)
        .take_while(|(left, right)| left == right)
        .count();
    let suffix = old_lines[prefix..]
        .iter()
        .rev()
        .zip(new_lines[prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    let old_end = old_lines.len().saturating_sub(suffix);
    let new_end = new_lines.len().saturating_sub(suffix);
    let mut result = vec![
        "--- earlier evidence".to_owned(),
        "+++ newer evidence".to_owned(),
    ];
    let context_start = prefix.saturating_sub(2);
    result.extend(
        old_lines[context_start..prefix]
            .iter()
            .map(|line| format!("  {line}")),
    );
    let removed = &old_lines[prefix..old_end];
    let added = &new_lines[prefix..new_end];
    result.extend(
        removed
            .iter()
            .take(MAX_CHANGED_LINES)
            .map(|line| format!("- {line}")),
    );
    if removed.len() > MAX_CHANGED_LINES {
        result.push(format!(
            "- … {} additional removed lines omitted",
            removed.len() - MAX_CHANGED_LINES
        ));
    }
    result.extend(
        added
            .iter()
            .take(MAX_CHANGED_LINES)
            .map(|line| format!("+ {line}")),
    );
    if added.len() > MAX_CHANGED_LINES {
        result.push(format!(
            "+ … {} additional added lines omitted",
            added.len() - MAX_CHANGED_LINES
        ));
    }
    result.extend(
        old_lines[old_end..]
            .iter()
            .take(2)
            .map(|line| format!("  {line}")),
    );
    result.join("\n")
}

pub fn build_alert(
    record: &EvidenceRecord,
    finding: &Finding,
    diff: Option<EvidenceDiff>,
) -> Option<Alert> {
    if finding.citations.is_empty() {
        return None;
    }
    let summary = diff
        .as_ref()
        .and_then(|item| item.changes.first())
        .map_or_else(
            || finding.classification_reason.clone(),
            |change| change.summary.clone(),
        );
    let previous = diff.as_ref().map(|item| item.old_evidence_id.clone());
    let publication_date = record
        .publication_date
        .clone()
        .unwrap_or_else(|| record.retrieval_timestamp.chars().take(10).collect());
    let id = stable_id(
        "alert",
        &[
            &record.id,
            previous.as_deref().unwrap_or(""),
            finding.state.label(),
            &finding.matched_rule_ids.join(","),
            &finding.rules_digest,
        ],
    );
    Some(Alert {
        id,
        jurisdiction: record.jurisdiction.clone(),
        evidence_id: record.id.clone(),
        previous_evidence_id: previous,
        title: record.document_title.clone(),
        state: finding.state,
        summary,
        publication_date,
        rule_ids: finding.matched_rule_ids.clone(),
        rules_version: finding.rules_version,
        rules_digest: finding.rules_digest.clone(),
        citations: finding.citations.clone(),
        diff,
    })
}

pub fn rules_digest(rules: &RuleSet) -> String {
    let bytes = serde_yaml::to_string(rules).unwrap_or_default();
    hex_digest(Sha256::digest(bytes.as_bytes()).as_slice())
}

fn hex_digest(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(char::from(DIGITS[usize::from(byte >> 4)]));
        result.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    result
}

/// Determine the role a document plays within a matter from its source type and
/// text. Conservative: a specific role is returned only when clearly
/// identifiable, otherwise [`DocumentRole::Other`].
pub fn document_role(record: &EvidenceRecord, text: &str) -> DocumentRole {
    let lower = text.to_lowercase();
    if lower.contains("be it ordained")
        || lower.contains("ordinance no.")
        || lower.contains("ordained by")
    {
        return DocumentRole::Ordinance;
    }
    if is_presentation(text) {
        return DocumentRole::Presentation;
    }
    if record.source_type == SourceType::Agenda || lower.contains("action:") {
        return DocumentRole::Agenda;
    }
    if lower.contains("minutes") && (lower.contains("roll call") || lower.contains("adopted")) {
        return DocumentRole::Minutes;
    }
    match record.source_type {
        SourceType::Contract => DocumentRole::Contract,
        SourceType::Amendment => DocumentRole::Amendment,
        SourceType::Pdf | SourceType::HtmlPage | SourceType::PlainText => {
            if lower.contains("solicitation") || lower.contains("request for proposals") {
                DocumentRole::Solicitation
            } else if lower.contains("policy") {
                DocumentRole::Policy
            } else {
                DocumentRole::Other
            }
        }
        SourceType::OfficialApi | SourceType::Agenda => DocumentRole::Other,
    }
}

fn is_presentation(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("presentation")
        || lower.contains("slide")
        || lower.contains("work session")
        || lower.contains("informational briefing")
}

/// Extract the institutional subjects and actions bound to a matter from a single
/// evidence record. The primary subject (the matter itself) and any vendor or
/// surveillance-technology mentions are modeled explicitly. A dispositive action
/// is only asserted when an explicit phrase in this same text supports it, so a
/// vendor mention in a supporting document can never become a purchase assertion.
pub fn extract_matter_subjects_and_actions(
    matter_id: &str,
    record: &EvidenceRecord,
    text: &str,
    rules: &RuleSet,
) -> (Vec<Subject>, Vec<Action>) {
    let labels = citation_labels(scan(record, text, rules).as_ref(), text);

    let mut subjects = Vec::new();
    let mut actions = Vec::new();

    let role = document_role(record, text);
    if let Some(primary) = primary_subject(matter_id, record, text, &role, labels.clone()) {
        actions.extend(classify_matter_actions(matter_id, &primary, text));
        subjects.push(primary);
    }

    for (subject, action) in vendor_subjects_and_actions(matter_id, text, rules, &labels) {
        subjects.push(subject);
        actions.push(action);
    }

    dedupe_by_id(&mut subjects, |subject| &subject.id);
    dedupe_by_id(&mut actions, |action| &action.id);

    (subjects, actions)
}

/// Reuse the line-based locator labels from a finding, or fall back to a single
/// "document" label when no finding citations are available.
fn citation_labels(finding: Option<&Finding>, text: &str) -> Vec<String> {
    if let Some(finding) = finding {
        let labels: Vec<String> = finding
            .citations
            .iter()
            .map(|item| item.locator.label.clone())
            .collect();
        if !labels.is_empty() {
            return labels;
        }
    }
    if text.lines().next().is_some() {
        vec!["document".to_owned()]
    } else {
        Vec::new()
    }
}

/// Identify the primary subject (the matter itself) and its conservative kind.
/// Returns `None` when no role or kind can be inferred, so nothing is asserted.
fn primary_subject(
    matter_id: &str,
    record: &EvidenceRecord,
    text: &str,
    role: &DocumentRole,
    citations: Vec<String>,
) -> Option<Subject> {
    let kind = match role {
        DocumentRole::Ordinance => SubjectKind::Ordinance,
        DocumentRole::Policy => SubjectKind::Policy,
        DocumentRole::Solicitation => SubjectKind::Solicitation,
        DocumentRole::Contract | DocumentRole::Award => SubjectKind::Contract,
        DocumentRole::Amendment => SubjectKind::Amendment,
        DocumentRole::StaffReport => SubjectKind::Program,
        DocumentRole::Presentation
        | DocumentRole::Agenda
        | DocumentRole::Minutes
        | DocumentRole::Other => infer_primary_kind(record, text)?,
    };
    let name = primary_name(record, text);
    let known = !matches!(kind, SubjectKind::Other | SubjectKind::Unknown);
    Some(Subject {
        id: Subject::id_for(matter_id, &name),
        matter_id: matter_id.to_owned(),
        kind,
        name: name.clone(),
        detail: String::new(),
        citations,
        known,
    })
}

fn infer_primary_kind(record: &EvidenceRecord, text: &str) -> Option<SubjectKind> {
    let lower = text.to_lowercase();
    if lower.contains("ordinance") || lower.contains("be it ordained") {
        Some(SubjectKind::Ordinance)
    } else if lower.contains("solicitation") || lower.contains("request for proposals") {
        Some(SubjectKind::Solicitation)
    } else if lower.contains("policy") {
        Some(SubjectKind::Policy)
    } else if record.document_title.to_lowercase().contains("ordinance") {
        Some(SubjectKind::Ordinance)
    } else {
        None
    }
}

fn primary_name(record: &EvidenceRecord, text: &str) -> String {
    const IDENTIFIERS: &[&str] = &[
        r"(?i)ordinance\s+no\.?\s+[\w.\-]+",
        r"(?i)resolution\s+no\.?\s+[\w.\-]+",
        r"(?i)measure\s+[\w.\-]+",
    ];
    for pattern in IDENTIFIERS {
        if let Ok(re) = Regex::new(pattern)
            && let Some(mat) = re.find(text)
        {
            return mat.as_str().to_owned();
        }
    }
    if !record.document_title.trim().is_empty() {
        return record.document_title.trim().to_owned();
    }
    "matter".to_owned()
}

/// Classify the dispositive action on the primary subject from a single text.
/// Returns the single strongest action when one is supported by an explicit
/// phrase, or an empty list when the text asserts conflicting outcomes (e.g.
/// both approval and rejection), so no stronger state is asserted without review.
pub fn classify_matter_actions(matter_id: &str, primary: &Subject, text: &str) -> Vec<Action> {
    const PRIORITY: &[(ActionKind, &[&str], &str)] = &[
        (
            ActionKind::Rejected,
            &["rejected", "denied", "failed by a vote"],
            "The source records rejection, denial, or a failed vote of the primary matter.",
        ),
        (
            ActionKind::Approved,
            &["finally passed", "approved", "adopted", "authorized"],
            "The source records final approval, adoption, authorization, or final passage of the primary matter.",
        ),
        (
            ActionKind::DeploymentReported,
            &["deployed", "in operation", "currently using"],
            "The source reports deployment or current operation of the primary matter.",
        ),
        (
            ActionKind::PolicyChanged,
            &["policy change", "policy amended", "policy revised"],
            "The source describes a policy change.",
        ),
        (
            ActionKind::VoteScheduled,
            &["vote scheduled", "will vote", "second reading"],
            "The source schedules a vote on the primary matter.",
        ),
        (
            ActionKind::HearingScheduled,
            &[
                "public hearing scheduled",
                "scheduled for hearing",
                "public hearing on",
            ],
            "The source schedules a public hearing on the primary matter.",
        ),
        (
            ActionKind::Proposed,
            &[
                "proposal",
                "proposed",
                "referred",
                "request for information",
            ],
            "The source describes the primary matter as a proposal or referral.",
        ),
    ];
    let detected: Vec<(ActionKind, &str)> = PRIORITY
        .iter()
        .filter(|(_, phrases, _)| {
            phrases.iter().any(|phrase| {
                find_term_lines(text, phrase).into_iter().any(|index| {
                    let line = text.lines().nth(index).unwrap_or_default();
                    !is_negated_or_conditional(line, phrase)
                })
            })
        })
        .map(|(kind, _, summary)| (kind.clone(), *summary))
        .collect();
    let approved = detected
        .iter()
        .any(|(kind, _)| *kind == ActionKind::Approved);
    let rejected = detected
        .iter()
        .any(|(kind, _)| *kind == ActionKind::Rejected);
    if approved && rejected {
        return Vec::new();
    }
    detected
        .into_iter()
        .take(1)
        .map(|(kind, summary)| Action {
            id: Action::id_for(matter_id, &primary.id, kind.clone(), summary),
            matter_id: matter_id.to_owned(),
            subject_id: primary.id.clone(),
            kind,
            summary: summary.to_owned(),
            citations: primary.citations.clone(),
            known: true,
        })
        .collect()
}

/// Produce a subject (and its single action) for every vendor or surveillance
/// technology that the rules match in the text. Dispositive procurement actions
/// are only emitted when an explicit phrase in the same text binds them to the
/// named vendor; otherwise the mention stays `ActionKind::Mentioned`.
fn vendor_subjects_and_actions(
    matter_id: &str,
    text: &str,
    rules: &RuleSet,
    citations: &[String],
) -> Vec<(Subject, Action)> {
    let mut result = Vec::new();
    for rule in &rules.rules {
        let kind = match rule.category.as_str() {
            "vendor" => SubjectKind::Vendor,
            "technology" => SubjectKind::SurveillanceTechnology,
            _ => continue,
        };
        if !rule_matches(rule, text) {
            continue;
        }
        let subject = Subject {
            id: Subject::id_for(matter_id, &rule.label),
            matter_id: matter_id.to_owned(),
            kind,
            name: rule.label.clone(),
            detail: String::new(),
            citations: citations.to_vec(),
            known: false,
        };
        let (action_kind, summary, known) = vendor_action(text, &subject);
        let action = Action {
            id: Action::id_for(matter_id, &subject.id, action_kind.clone(), &summary),
            matter_id: matter_id.to_owned(),
            subject_id: subject.id.clone(),
            kind: action_kind,
            summary,
            citations: citations.to_vec(),
            known,
        };
        result.push((subject, action));
    }
    result
}

fn rule_matches(rule: &Rule, text: &str) -> bool {
    rule.terms.iter().any(|term| {
        find_term_lines(text, term).into_iter().any(|index| {
            let line = text.lines().nth(index).unwrap_or_default().to_lowercase();
            !rule
                .false_positive_phrases
                .iter()
                .any(|phrase| line.contains(&phrase.to_lowercase()))
        })
    })
}

/// Determine the single action on a vendor/technology subject. A dispositive
/// procurement action is returned only when an explicit phrase in the same text
/// binds that action to the named vendor; a bare mention stays `Mentioned` with
/// `known == false`.
fn vendor_action(text: &str, subject: &Subject) -> (ActionKind, String, bool) {
    const CANDIDATES: &[(ActionKind, &[&str])] = &[
        (
            ActionKind::Renewed,
            &[
                "renewed with {name}",
                "renewal of {name}",
                "renewed {name} contract",
            ],
        ),
        (
            ActionKind::Expanded,
            &[
                "expanded {name}",
                "expansion of {name}",
                "expanded use of {name}",
            ],
        ),
        (
            ActionKind::Executed,
            &[
                "contract with {name}",
                "agreement with {name}",
                "entered into a contract with {name}",
                "signed a contract with {name}",
            ],
        ),
        (
            ActionKind::Approved,
            &[
                "approved purchase of {name}",
                "approved the purchase of {name}",
                "approved a contract with {name}",
            ],
        ),
        (
            ActionKind::Awarded,
            &[
                "awarded to {name}",
                "award to {name}",
                "contract awarded to {name}",
                "awarded {name}",
            ],
        ),
    ];
    let lower = text.to_lowercase();
    let name = subject.name.to_lowercase();
    let bound = |phrases: &[&str]| {
        phrases
            .iter()
            .any(|phrase| lower.contains(&phrase.replace("{name}", &name)))
    };
    for (kind, phrases) in CANDIDATES {
        if bound(phrases) {
            return (
                kind.clone(),
                format!(
                    "The source explicitly records a {} of the vendor.",
                    kind.clone().label().to_lowercase()
                ),
                true,
            );
        }
    }
    (
        ActionKind::Mentioned,
        "The vendor appears in the source, but no explicit dispositive phrase binds a procurement action to it."
            .to_owned(),
        false,
    )
}

fn dedupe_by_id<T>(items: &mut Vec<T>, key: impl Fn(&T) -> &str) {
    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(key(item).to_owned()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use pnull_core::{ExtractionStatus, SourceType};
    use std::path::PathBuf;

    fn record(id: &str) -> EvidenceRecord {
        EvidenceRecord {
            id: id.to_owned(),
            jurisdiction: "Colorado Springs, Colorado".to_owned(),
            source_url: format!("https://example.test/{id}"),
            source_type: SourceType::Agenda,
            document_title: "Agenda".to_owned(),
            publication_date: Some("2025-01-01".to_owned()),
            retrieval_timestamp: "2025-01-01T00:00:00Z".to_owned(),
            mime_type: "text/plain".to_owned(),
            sha256: "00".repeat(32),
            original_filename: "agenda.txt".to_owned(),
            extraction_method: "test".to_owned(),
            extraction_status: ExtractionStatus::Complete,
            extraction_error: None,
            locators: Vec::new(),
            matched_rule_ids: Vec::new(),
            quoted_source_spans: Vec::new(),
            supersedes: None,
            processing_version: "test".to_owned(),
        }
    }

    fn rules() -> RuleSet {
        RuleSet {
            version: 1,
            rules: vec![Rule {
                id: "vendor.axon".to_owned(),
                label: "Axon".to_owned(),
                category: "vendor".to_owned(),
                terms: vec!["Axon".to_owned()],
                false_positive_phrases: vec!["axon of a neuron".to_owned()],
                rationale: "Vendor term".to_owned(),
            }],
        }
    }

    #[test]
    fn keyword_alone_is_only_a_mention() {
        let finding = scan(&record("one"), "The document names Axon.", &rules()).expect("finding");
        assert_eq!(finding.state, FindingState::MentionDetected);
        assert!(finding.classification_reason.contains("does not establish"));
    }

    #[test]
    fn explicit_signed_contract_has_explanation_and_citation() {
        let finding = scan(
            &record("one"),
            "The city signed a ten-year contract with Axon.",
            &rules(),
        )
        .expect("finding");
        assert_eq!(finding.state, FindingState::ContractExecuted);
        assert!(!finding.citations.is_empty());
    }

    #[test]
    fn documented_false_positive_does_not_match() {
        assert!(
            scan(
                &record("one"),
                "The axon of a neuron carries a signal.",
                &rules()
            )
            .is_none()
        );
    }

    #[test]
    fn negated_approval_never_becomes_approved() {
        let finding =
            scan(&record("one"), "Axon proposal was not approved.", &rules()).expect("mention");
        assert_ne!(finding.state, FindingState::Approved);
    }

    #[test]
    fn false_positive_elsewhere_does_not_hide_real_vendor_reference() {
        let text = "A biology appendix mentions the axon of a neuron.\nThe city proposed an Axon body camera agreement.";
        let finding = scan(&record("one"), text, &rules()).expect("real vendor finding");
        assert_eq!(finding.state, FindingState::Proposal);
    }

    #[test]
    fn detects_price_and_privacy_language_removal() {
        let old = "Axon price: $10,000\nRetention shall be 30 days\nPrivacy review required";
        let new = "Axon price: $20,000\nRetention shall be 90 days";
        let diff = compare(&record("old"), old, &record("new"), new, &rules());
        assert!(
            diff.changes
                .iter()
                .any(|change| change.kind == "changed_price")
        );
        assert!(
            diff.changes
                .iter()
                .any(|change| change.kind == "changed_retention_provision")
        );
        assert!(
            diff.changes
                .iter()
                .any(|change| change.kind == "removed_relevant_language")
        );
    }

    #[test]
    fn diff_locators_handle_matches_at_byte_zero() {
        let diff = compare(
            &record("old"),
            "$10 Axon",
            &record("new"),
            "$20 Axon",
            &rules(),
        );
        let price = diff
            .changes
            .iter()
            .find(|change| change.kind == "changed_price")
            .expect("price change");
        assert_eq!(price.before.as_ref().expect("before").locator.start, 1);
        assert_eq!(price.after.as_ref().expect("after").locator.start, 1);
    }

    #[test]
    fn textual_diff_preserves_duplicate_line_changes() {
        let rendered = unified_text("Axon\nAxon\nend", "Axon\nend");
        assert!(rendered.contains("- Axon"));
    }

    #[test]
    fn detects_agenda_state_and_date_change() {
        let old = "Meeting date: 2025-01-01\nAction: referred\nAxon proposal";
        let new = "Meeting date: 2025-02-01\nAction: finally passed\nAxon proposal";
        let diff = compare(&record("old"), old, &record("new"), new, &rules());
        assert!(
            diff.changes
                .iter()
                .any(|change| change.kind == "changed_vote_date")
        );
        assert!(
            diff.changes
                .iter()
                .any(|change| change.kind == "changed_procurement_state")
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn ordinance_approval_never_becomes_a_vendor_purchase() {
        let rules_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("rules/surveillance.yml");
        let rules = load_rules(&rules_path).expect("load surveillance rules");

        // Record A: the signed ordinance itself. No vendor is named.
        let ordinance_text = "\
ORDINANCE NO. 25-93
BE IT ORDAINED BY THE CITY COUNCIL OF THE CITY OF COLORADO SPRINGS:
SECTION 1. The City Council hereby finally passed Ordinance No. 25-93,
which authorizes the continued operation of police surveillance technology.
This program is funded by a police department technology surcharge.";
        // Record B: a supporting presentation that names vendors with costs.
        let presentation_text = "\
Council Work Session Presentation
The proposed program may use Axon body-worn cameras and Flock Safety
automated license plate readers.
Estimated costs: Axon $1,200,000 and Flock Safety $500,000.";

        let combined_text = format!("{ordinance_text}\n\n{presentation_text}");
        let record = EvidenceRecord {
            id: "evidence:ord-25-93-combined".to_owned(),
            jurisdiction: "Colorado Springs, Colorado".to_owned(),
            source_url: "https://example.test/ordinance-25-93".to_owned(),
            source_type: SourceType::Pdf,
            document_title: "Ordinance No. 25-93".to_owned(),
            publication_date: Some("2025-01-01".to_owned()),
            retrieval_timestamp: "2025-01-01T00:00:00Z".to_owned(),
            mime_type: "application/pdf".to_owned(),
            sha256: "00".repeat(32),
            original_filename: "ordinance-25-93-signed.pdf".to_owned(),
            extraction_method: "test".to_owned(),
            extraction_status: ExtractionStatus::Complete,
            extraction_error: None,
            locators: Vec::new(),
            matched_rule_ids: Vec::new(),
            quoted_source_spans: Vec::new(),
            supersedes: None,
            processing_version: "test".to_owned(),
        };

        let (subjects, actions) =
            extract_matter_subjects_and_actions("matter:25-93", &record, &combined_text, &rules);

        // The ordinance is the primary subject and it was approved.
        let ordinance = subjects
            .iter()
            .find(|subject| subject.kind == SubjectKind::Ordinance)
            .expect("ordinance subject");
        assert!(
            actions
                .iter()
                .any(|action| action.subject_id == ordinance.id
                    && action.kind == ActionKind::Approved),
            "the ordinance should carry an Approved action"
        );

        // Axon and Flock Safety are vendor mentions in the supporting material.
        let axon = subjects
            .iter()
            .find(|subject| subject.name.contains("Axon"))
            .expect("Axon vendor subject");
        let flock = subjects
            .iter()
            .find(|subject| subject.name.contains("Flock"))
            .expect("Flock Safety vendor subject");

        for vendor in [axon, flock] {
            let vendor_actions: Vec<&Action> = actions
                .iter()
                .filter(|action| action.subject_id == vendor.id)
                .collect();
            assert!(
                !vendor_actions.is_empty(),
                "{} should be mentioned",
                vendor.name
            );
            for action in &vendor_actions {
                assert_eq!(
                    action.kind,
                    ActionKind::Mentioned,
                    "{} must only be Mentioned, not a procurement",
                    vendor.name
                );
                assert!(
                    !action.known,
                    "{} mention must have known == false",
                    vendor.name
                );
            }
            // No dispositive action transfers the ordinance approval onto a vendor.
            assert!(
                vendor_actions.iter().all(|action| !matches!(
                    action.kind,
                    ActionKind::Approved
                        | ActionKind::Awarded
                        | ActionKind::Executed
                        | ActionKind::Renewed
                        | ActionKind::Expanded
                )),
                "{} must not be Approved/Awarded/Executed/Renewed/Expanded",
                vendor.name
            );
        }

        // Every approval targets the ordinance subject, never a vendor.
        for action in actions
            .iter()
            .filter(|action| action.kind == ActionKind::Approved)
        {
            assert_eq!(
                action.subject_id, ordinance.id,
                "the approval must target the ordinance only"
            );
        }
    }
}
