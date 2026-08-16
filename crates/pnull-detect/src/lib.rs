//! Transparent deterministic surveillance detection and document comparison.

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::Path;

use pnull_core::{
    Alert, Citation, DiffChange, EvidenceDiff, EvidenceRecord, Finding, FindingState, Locator,
    stable_id,
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
    let lowercase = text.to_lowercase();
    let mut matched = BTreeSet::new();
    let mut citations = Vec::new();
    for rule in &rules.rules {
        if rule
            .false_positive_phrases
            .iter()
            .any(|phrase| lowercase.contains(&phrase.to_lowercase()))
        {
            continue;
        }
        for term in &rule.terms {
            if let Some(line_index) = find_term_line(text, term) {
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
    let id = stable_id(
        "finding",
        &[&record.id, state.label(), &matched_rule_ids.join(",")],
    );
    Some(Finding {
        id,
        evidence_id: record.id.clone(),
        jurisdiction: record.jurisdiction.clone(),
        state,
        classification_reason: reason,
        matched_rule_ids,
        citations,
    })
}

fn find_term_line(text: &str, term: &str) -> Option<usize> {
    let escaped = regex::escape(term);
    let starts_word = term.chars().next().is_some_and(char::is_alphanumeric);
    let ends_word = term.chars().last().is_some_and(char::is_alphanumeric);
    let pattern = format!(
        "(?i){}{}{}",
        if starts_word { r"\b" } else { "" },
        escaped,
        if ends_word { r"\b" } else { "" }
    );
    let regex = Regex::new(&pattern).ok()?;
    text.lines().position(|line| regex.is_match(line))
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
    for (state, phrases, reason) in PATTERNS {
        for phrase in *phrases {
            if let Some(index) = find_term_line(text, phrase)
                && accepts(index)
            {
                return (
                    *state,
                    (*reason).to_owned(),
                    Some(citation(record, text, index)),
                );
            }
        }
    }
    (
        FindingState::MentionDetected,
        "A taxonomy term appears, but the source span does not establish proposal, approval, purchase, deployment, or another stronger state.".to_owned(),
        None,
    )
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
    EvidenceDiff {
        old_evidence_id: old_record.id.clone(),
        new_evidence_id: new_record.id.clone(),
        old_source_url: old_record.source_url.clone(),
        new_source_url: new_record.source_url.clone(),
        changes,
        unified_text: unified_text(old_text, new_text),
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
            before: regex.find(old_text).map(|item| {
                citation(
                    old_record,
                    old_text,
                    old_text[..item.start()].lines().count() - 1,
                )
            }),
            after: regex.find(new_text).map(|item| {
                citation(
                    new_record,
                    new_text,
                    new_text[..item.start()].lines().count() - 1,
                )
            }),
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

fn unified_text(old: &str, new: &str) -> String {
    let old_lines: BTreeSet<&str> = old.lines().collect();
    let new_lines: BTreeSet<&str> = new.lines().collect();
    let mut result = vec!["--- earlier evidence", "+++ newer evidence"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    result.extend(
        old_lines
            .difference(&new_lines)
            .map(|line| format!("- {line}")),
    );
    result.extend(
        new_lines
            .difference(&old_lines)
            .map(|line| format!("+ {line}")),
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

#[cfg(test)]
mod tests {
    use super::*;
    use pnull_core::{ExtractionStatus, SourceType};

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
}
