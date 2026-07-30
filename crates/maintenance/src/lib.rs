use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum MaintenanceFindingKind {
    MissingMedia,
    LicenseReviewRequired,
    StaleWaveformCache,
    DuplicateContent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MaintenanceAction {
    Relink,
    Review,
    Regenerate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MaintenanceSeverity {
    Ok,
    Warning,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MaintenanceFinding {
    pub kind: MaintenanceFindingKind,
    pub asset_ids: Vec<Uuid>,
    pub detail: String,
    pub recommended_action: MaintenanceAction,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MaintenanceReport {
    pub total_findings: usize,
    pub severity: MaintenanceSeverity,
    pub counts_by_kind: BTreeMap<MaintenanceFindingKind, usize>,
    pub findings: Vec<MaintenanceFinding>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PreviewCacheEntry {
    pub asset_id: Uuid,
    pub path: String,
    pub size_bytes: u64,
    pub last_accessed_at_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PreviewCacheEvictionPlan {
    pub limit_bytes: u64,
    pub current_size_bytes: u64,
    pub bytes_to_free: u64,
    pub candidates: Vec<PreviewCacheEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PreviewCacheEvictionResult {
    pub removed_paths: Vec<String>,
    pub failed_paths: Vec<String>,
    pub bytes_freed: u64,
}

impl MaintenanceFinding {
    pub fn missing_media(asset_id: Uuid) -> Self {
        Self {
            kind: MaintenanceFindingKind::MissingMedia,
            asset_ids: vec![asset_id],
            detail: "Original media is unavailable".to_string(),
            recommended_action: MaintenanceAction::Relink,
        }
    }

    pub fn license_review_required(asset_id: Uuid) -> Self {
        Self {
            kind: MaintenanceFindingKind::LicenseReviewRequired,
            asset_ids: vec![asset_id],
            detail: "Source or license status needs review".to_string(),
            recommended_action: MaintenanceAction::Review,
        }
    }

    pub fn stale_waveform_cache(asset_id: Uuid) -> Self {
        Self {
            kind: MaintenanceFindingKind::StaleWaveformCache,
            asset_ids: vec![asset_id],
            detail: "Waveform cache should be regenerated".to_string(),
            recommended_action: MaintenanceAction::Regenerate,
        }
    }

    pub fn duplicate_content(content_hash: String, asset_ids: Vec<Uuid>) -> Self {
        Self {
            kind: MaintenanceFindingKind::DuplicateContent,
            asset_ids,
            detail: format!("Duplicate content hash: {content_hash}"),
            recommended_action: MaintenanceAction::Review,
        }
    }
}

pub fn plan_preview_cache_eviction(
    entries: impl IntoIterator<Item = PreviewCacheEntry>,
    limit_bytes: u64,
) -> PreviewCacheEvictionPlan {
    let mut entries = entries.into_iter().collect::<Vec<_>>();
    let current_size_bytes = entries
        .iter()
        .map(|entry| entry.size_bytes)
        .fold(0_u64, u64::saturating_add);
    let bytes_to_free = current_size_bytes.saturating_sub(limit_bytes);

    entries.sort_by(|left, right| {
        left.last_accessed_at_ms
            .cmp(&right.last_accessed_at_ms)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.asset_id.cmp(&right.asset_id))
    });

    let mut planned_free_bytes = 0_u64;
    let mut candidates = Vec::new();

    if bytes_to_free > 0 {
        for entry in entries {
            planned_free_bytes = planned_free_bytes.saturating_add(entry.size_bytes);
            candidates.push(entry);

            if planned_free_bytes >= bytes_to_free {
                break;
            }
        }
    }

    PreviewCacheEvictionPlan {
        limit_bytes,
        current_size_bytes,
        bytes_to_free,
        candidates,
    }
}

/// Removes each planned eviction candidate via `remove`, continuing past failures so one
/// locked or already-missing file cannot block reclaiming space from the rest.
pub fn evict_preview_cache(
    plan: &PreviewCacheEvictionPlan,
    mut remove: impl FnMut(&str) -> bool,
) -> PreviewCacheEvictionResult {
    let mut removed_paths = Vec::new();
    let mut failed_paths = Vec::new();
    let mut bytes_freed = 0_u64;

    for candidate in &plan.candidates {
        if remove(&candidate.path) {
            removed_paths.push(candidate.path.clone());
            bytes_freed = bytes_freed.saturating_add(candidate.size_bytes);
        } else {
            failed_paths.push(candidate.path.clone());
        }
    }

    PreviewCacheEvictionResult {
        removed_paths,
        failed_paths,
        bytes_freed,
    }
}

impl MaintenanceReport {
    pub fn from_findings(findings: Vec<MaintenanceFinding>) -> Self {
        let mut counts_by_kind = BTreeMap::new();
        for finding in &findings {
            *counts_by_kind.entry(finding.kind).or_insert(0) += 1;
        }

        let total_findings = findings.len();
        Self {
            total_findings,
            severity: if total_findings == 0 {
                MaintenanceSeverity::Ok
            } else {
                MaintenanceSeverity::Warning
            },
            counts_by_kind,
            findings,
        }
    }

    pub fn count_for(&self, kind: MaintenanceFindingKind) -> usize {
        self.counts_by_kind.get(&kind).copied().unwrap_or(0)
    }
}

impl PreviewCacheEvictionPlan {
    pub fn planned_free_bytes(&self) -> u64 {
        self.candidates
            .iter()
            .map(|entry| entry.size_bytes)
            .fold(0_u64, u64::saturating_add)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn maintenance_report_counts_missing_license_and_waveform_issues() {
        let missing = Uuid::new_v4();
        let license = Uuid::new_v4();
        let waveform = Uuid::new_v4();

        let report = MaintenanceReport::from_findings(vec![
            MaintenanceFinding::missing_media(missing),
            MaintenanceFinding::license_review_required(license),
            MaintenanceFinding::stale_waveform_cache(waveform),
        ]);

        assert_eq!(report.total_findings, 3);
        assert_eq!(report.count_for(MaintenanceFindingKind::MissingMedia), 1);
        assert_eq!(
            report.count_for(MaintenanceFindingKind::LicenseReviewRequired),
            1
        );
        assert_eq!(
            report.count_for(MaintenanceFindingKind::StaleWaveformCache),
            1
        );
        assert_eq!(report.severity, MaintenanceSeverity::Warning);
    }

    #[test]
    fn duplicate_content_is_reported_without_destructive_action() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let finding =
            MaintenanceFinding::duplicate_content("hash-a".to_string(), vec![first, second]);

        assert_eq!(finding.kind, MaintenanceFindingKind::DuplicateContent);
        assert_eq!(finding.recommended_action, MaintenanceAction::Review);
        assert_eq!(finding.asset_ids, vec![first, second]);
    }

    #[test]
    fn clean_report_has_ok_severity() {
        let report = MaintenanceReport::from_findings(Vec::new());

        assert_eq!(report.total_findings, 0);
        assert_eq!(report.severity, MaintenanceSeverity::Ok);
    }

    #[test]
    fn preview_cache_eviction_plans_least_recently_used_files_until_under_limit() {
        let oldest = Uuid::new_v4();
        let newer = Uuid::new_v4();
        let newest = Uuid::new_v4();
        let entries = vec![
            PreviewCacheEntry {
                asset_id: newest,
                path: "cache/newest.wav".to_string(),
                size_bytes: 300,
                last_accessed_at_ms: 30,
            },
            PreviewCacheEntry {
                asset_id: oldest,
                path: "cache/oldest.wav".to_string(),
                size_bytes: 400,
                last_accessed_at_ms: 10,
            },
            PreviewCacheEntry {
                asset_id: newer,
                path: "cache/newer.wav".to_string(),
                size_bytes: 500,
                last_accessed_at_ms: 20,
            },
        ];

        let plan = plan_preview_cache_eviction(entries, 700);

        assert_eq!(plan.current_size_bytes, 1_200);
        assert_eq!(plan.limit_bytes, 700);
        assert_eq!(plan.bytes_to_free, 500);
        assert_eq!(
            plan.candidates
                .iter()
                .map(|candidate| candidate.asset_id)
                .collect::<Vec<_>>(),
            vec![oldest, newer]
        );
        assert_eq!(plan.planned_free_bytes(), 900);
    }

    #[test]
    fn preview_cache_eviction_is_empty_when_cache_is_under_limit() {
        let plan = plan_preview_cache_eviction(
            vec![PreviewCacheEntry {
                asset_id: Uuid::new_v4(),
                path: "cache/preview.wav".to_string(),
                size_bytes: 512,
                last_accessed_at_ms: 10,
            }],
            700,
        );

        assert_eq!(plan.current_size_bytes, 512);
        assert_eq!(plan.bytes_to_free, 0);
        assert!(plan.candidates.is_empty());
    }

    #[test]
    fn evict_preview_cache_removes_candidates_and_sums_bytes_freed() {
        let plan = PreviewCacheEvictionPlan {
            limit_bytes: 700,
            current_size_bytes: 1_200,
            bytes_to_free: 500,
            candidates: vec![
                PreviewCacheEntry {
                    asset_id: Uuid::new_v4(),
                    path: "cache/oldest.wav".to_string(),
                    size_bytes: 400,
                    last_accessed_at_ms: 10,
                },
                PreviewCacheEntry {
                    asset_id: Uuid::new_v4(),
                    path: "cache/newer.wav".to_string(),
                    size_bytes: 500,
                    last_accessed_at_ms: 20,
                },
            ],
        };

        let result = evict_preview_cache(&plan, |_path| true);

        assert_eq!(
            result.removed_paths,
            vec![
                "cache/oldest.wav".to_string(),
                "cache/newer.wav".to_string()
            ]
        );
        assert!(result.failed_paths.is_empty());
        assert_eq!(result.bytes_freed, 900);
    }

    #[test]
    fn evict_preview_cache_tracks_failures_without_stopping() {
        let plan = PreviewCacheEvictionPlan {
            limit_bytes: 700,
            current_size_bytes: 1_200,
            bytes_to_free: 500,
            candidates: vec![
                PreviewCacheEntry {
                    asset_id: Uuid::new_v4(),
                    path: "cache/locked.wav".to_string(),
                    size_bytes: 400,
                    last_accessed_at_ms: 10,
                },
                PreviewCacheEntry {
                    asset_id: Uuid::new_v4(),
                    path: "cache/newer.wav".to_string(),
                    size_bytes: 500,
                    last_accessed_at_ms: 20,
                },
            ],
        };

        let result = evict_preview_cache(&plan, |path| path != "cache/locked.wav");

        assert_eq!(result.removed_paths, vec!["cache/newer.wav".to_string()]);
        assert_eq!(result.failed_paths, vec!["cache/locked.wav".to_string()]);
        assert_eq!(result.bytes_freed, 500);
    }
}
