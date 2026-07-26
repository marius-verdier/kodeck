use std::collections::{BTreeMap, HashMap, HashSet};

use crate::domain::{AnnotationDisposition, AnnotationId, CardsFile, CodeAnnotation, Finding};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AnnotationSyncSummary {
    pub created: usize,
    pub updated: usize,
    pub disappeared: usize,
    pub archived_cards: usize,
    pub restored_cards: usize,
    pub present: usize,
    pub unassigned: usize,
}

pub fn reconcile_annotations(
    cards: &CardsFile,
    mut findings: Vec<Finding>,
) -> (CardsFile, AnnotationSyncSummary) {
    findings.sort_by(|left, right| {
        (
            left.repository_id.as_str(),
            left.path.as_str(),
            left.line,
            left.tag.as_str(),
            left.message.as_str(),
        )
            .cmp(&(
                right.repository_id.as_str(),
                right.path.as_str(),
                right.line,
                right.tag.as_str(),
                right.message.as_str(),
            ))
    });

    let mut observations = Vec::with_capacity(findings.len());
    let mut occurrences: HashMap<(String, String, String, String), usize> = HashMap::new();
    for finding in findings {
        let key = (
            finding.repository_id.as_str().to_owned(),
            finding.path.clone(),
            finding.tag.to_ascii_uppercase(),
            finding.message.clone(),
        );
        let occurrence = occurrences.entry(key).or_default();
        observations.push((finding, *occurrence));
        *occurrence += 1;
    }

    let mut next = cards.clone();
    let was_present: HashSet<_> = next
        .annotations
        .iter()
        .filter(|annotation| annotation.present)
        .map(|annotation| annotation.id)
        .collect();
    for annotation in &mut next.annotations {
        annotation.present = false;
    }

    let mut matched_annotations = HashSet::new();
    let mut matched_observations = HashSet::new();
    let mut summary = AnnotationSyncSummary::default();

    for (observation_index, (finding, occurrence)) in observations.iter().enumerate() {
        let Some((annotation_index, annotation)) =
            next.annotations
                .iter_mut()
                .enumerate()
                .find(|(_, annotation)| {
                    !matched_annotations.contains(&annotation.id)
                        && annotation.repository_id == finding.repository_id
                        && annotation.path == finding.path
                        && annotation.tag.eq_ignore_ascii_case(&finding.tag)
                        && annotation.message == finding.message
                        && annotation.occurrence == *occurrence
                })
        else {
            continue;
        };
        matched_annotations.insert(annotation.id);
        matched_observations.insert(observation_index);
        if annotation.line != finding.line || !was_present.contains(&annotation.id) {
            summary.updated += 1;
        }
        annotation.line = finding.line;
        annotation.present = true;
        let _ = annotation_index;
    }

    let mut old_groups: BTreeMap<(String, String, String), Vec<usize>> = BTreeMap::new();
    for (index, annotation) in next.annotations.iter().enumerate() {
        if matched_annotations.contains(&annotation.id)
            || !matches!(annotation.disposition, AnnotationDisposition::Linked { .. })
        {
            continue;
        }
        old_groups
            .entry((
                annotation.repository_id.as_str().to_owned(),
                annotation.path.clone(),
                annotation.tag.to_ascii_uppercase(),
            ))
            .or_default()
            .push(index);
    }
    let mut new_groups: BTreeMap<(String, String, String), Vec<usize>> = BTreeMap::new();
    for (index, (finding, _)) in observations.iter().enumerate() {
        if matched_observations.contains(&index) {
            continue;
        }
        new_groups
            .entry((
                finding.repository_id.as_str().to_owned(),
                finding.path.clone(),
                finding.tag.to_ascii_uppercase(),
            ))
            .or_default()
            .push(index);
    }
    for (key, old) in old_groups {
        let Some(new) = new_groups.get(&key) else {
            continue;
        };
        if let ([annotation_index], [observation_index]) = (old.as_slice(), new.as_slice()) {
            let (finding, occurrence) = &observations[*observation_index];
            let annotation = &mut next.annotations[*annotation_index];
            annotation.line = finding.line;
            annotation.message = finding.message.clone();
            annotation.occurrence = *occurrence;
            annotation.present = true;
            matched_annotations.insert(annotation.id);
            matched_observations.insert(*observation_index);
            summary.updated += 1;
        }
    }

    for (index, (finding, occurrence)) in observations.into_iter().enumerate() {
        if matched_observations.contains(&index) {
            continue;
        }
        next.annotations.push(CodeAnnotation {
            id: AnnotationId::generate(),
            repository_id: finding.repository_id,
            path: finding.path,
            line: finding.line,
            tag: finding.tag,
            message: finding.message,
            occurrence,
            present: true,
            disposition: AnnotationDisposition::Unassigned,
        });
        summary.created += 1;
    }

    summary.disappeared = next
        .annotations
        .iter()
        .filter(|annotation| was_present.contains(&annotation.id) && !annotation.present)
        .count();

    let linked: HashMap<_, Vec<_>> = next
        .annotations
        .iter()
        .filter_map(|annotation| match annotation.disposition {
            AnnotationDisposition::Linked { card_id } => Some((card_id, annotation.present)),
            AnnotationDisposition::Unassigned | AnnotationDisposition::Ignored => None,
        })
        .fold(
            HashMap::<_, Vec<_>>::new(),
            |mut grouped, (card_id, present)| {
                grouped.entry(card_id).or_default().push(present);
                grouped
            },
        );
    for card in &mut next.cards {
        let Some(sources) = linked.get(&card.id) else {
            continue;
        };
        let any_present = sources.iter().any(|present| *present);
        if !any_present && !card.archived {
            card.archived = true;
            card.archived_by_sync = true;
            for ids in next.ordering.values_mut() {
                ids.retain(|id| *id != card.id);
            }
            summary.archived_cards += 1;
        } else if any_present && card.archived && card.archived_by_sync {
            card.archived = false;
            card.archived_by_sync = false;
            next.ordering
                .entry(card.column_id.clone())
                .or_default()
                .push(card.id);
            summary.restored_cards += 1;
        }
    }

    summary.present = next
        .annotations
        .iter()
        .filter(|annotation| annotation.present)
        .count();
    summary.unassigned = next
        .annotations
        .iter()
        .filter(|annotation| {
            annotation.present
                && matches!(annotation.disposition, AnnotationDisposition::Unassigned)
        })
        .count();
    (next, summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        AnnotationDisposition, CardId, CodeAnnotation, ColumnId, LocalCard, RepositoryId,
        TaskPriority, WorkspaceId,
    };

    fn finding(line: usize, tag: &str, message: &str) -> Finding {
        Finding {
            repository_id: RepositoryId::from("repo"),
            path: "src/main.rs".to_owned(),
            line,
            tag: tag.to_owned(),
            message: message.to_owned(),
        }
    }

    fn annotation(
        id: AnnotationId,
        line: usize,
        message: &str,
        disposition: AnnotationDisposition,
    ) -> CodeAnnotation {
        CodeAnnotation {
            id,
            repository_id: RepositoryId::from("repo"),
            path: "src/main.rs".to_owned(),
            line,
            tag: "TODO".to_owned(),
            message: message.to_owned(),
            occurrence: 0,
            present: true,
            disposition,
        }
    }

    #[test]
    fn exact_annotation_survives_line_moves_without_duplication() {
        let id = AnnotationId::generate();
        let mut cards = CardsFile::empty(WorkspaceId::generate());
        cards.annotations.push(annotation(
            id,
            4,
            "keep identity",
            AnnotationDisposition::Unassigned,
        ));

        let (next, summary) =
            reconcile_annotations(&cards, vec![finding(40, "TODO", "keep identity")]);

        assert_eq!(next.annotations.len(), 1);
        assert_eq!(next.annotations[0].id, id);
        assert_eq!(next.annotations[0].line, 40);
        assert_eq!(summary.created, 0);
    }

    #[test]
    fn cautious_matching_updates_one_linked_annotation_but_not_an_ignored_one() {
        let card_id = CardId::generate();
        let linked_id = AnnotationId::generate();
        let ignored_id = AnnotationId::generate();
        let mut cards = CardsFile::empty(WorkspaceId::generate());
        cards.cards.push(LocalCard {
            id: card_id,
            title: "Managed".to_owned(),
            description: "Managed".to_owned(),
            priority: TaskPriority::LOW,
            column_id: ColumnId::from("inbox"),
            archived: false,
            archived_by_sync: false,
        });
        cards
            .ordering
            .insert(ColumnId::from("inbox"), vec![card_id]);
        cards.annotations.push(annotation(
            linked_id,
            4,
            "old linked",
            AnnotationDisposition::Linked { card_id },
        ));

        let (next, _) = reconcile_annotations(&cards, vec![finding(5, "TODO", "new linked")]);
        assert_eq!(next.annotations.len(), 1);
        assert_eq!(next.annotations[0].id, linked_id);
        assert_eq!(next.annotations[0].message, "new linked");

        let mut ignored = CardsFile::empty(WorkspaceId::generate());
        ignored.annotations.push(annotation(
            ignored_id,
            4,
            "old ignored",
            AnnotationDisposition::Ignored,
        ));
        let (next, _) = reconcile_annotations(&ignored, vec![finding(5, "TODO", "new ignored")]);
        assert_eq!(next.annotations.len(), 2);
        assert!(!next.annotations[0].present);
        assert!(next.annotations[1].present);
        assert!(matches!(
            next.annotations[1].disposition,
            AnnotationDisposition::Unassigned
        ));
    }

    #[test]
    fn cards_archive_when_all_sources_disappear_and_restore_only_after_sync_archives() {
        let card_id = CardId::generate();
        let annotation_id = AnnotationId::generate();
        let mut cards = CardsFile::empty(WorkspaceId::generate());
        cards.cards.push(LocalCard {
            id: card_id,
            title: "Managed".to_owned(),
            description: "Managed".to_owned(),
            priority: TaskPriority::LOW,
            column_id: ColumnId::from("inbox"),
            archived: false,
            archived_by_sync: false,
        });
        cards
            .ordering
            .insert(ColumnId::from("inbox"), vec![card_id]);
        cards.annotations.push(annotation(
            annotation_id,
            4,
            "managed",
            AnnotationDisposition::Linked { card_id },
        ));

        let (archived, summary) = reconcile_annotations(&cards, Vec::new());
        assert!(archived.cards[0].archived);
        assert!(archived.cards[0].archived_by_sync);
        assert_eq!(summary.archived_cards, 1);

        let (restored, summary) =
            reconcile_annotations(&archived, vec![finding(8, "TODO", "managed")]);
        assert!(!restored.cards[0].archived);
        assert!(!restored.cards[0].archived_by_sync);
        assert_eq!(summary.restored_cards, 1);

        let mut manual = archived;
        manual.cards[0].archived_by_sync = false;
        let (still_archived, _) =
            reconcile_annotations(&manual, vec![finding(8, "TODO", "managed")]);
        assert!(still_archived.cards[0].archived);
    }
}
