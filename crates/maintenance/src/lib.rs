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
}
