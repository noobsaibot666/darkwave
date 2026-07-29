use std::fs;
use std::io;
use std::path::Path;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportIntent {
    pub preserve_original: bool,
    pub include_license_record: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportPreset {
    Original,
    Wav48k24Bit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExportRangeMs {
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioConversion {
    pub sample_rate: u32,
    pub bit_depth: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportRequest {
    pub source_path: String,
    pub project_media_dir: String,
    pub asset_display_name: String,
    pub preset: ExportPreset,
    pub range: Option<ExportRangeMs>,
    pub intent: ExportIntent,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportPlan {
    pub source_path: String,
    pub destination_path: String,
    pub range: Option<ExportRangeMs>,
    pub conversion: Option<AudioConversion>,
    pub preserve_original: bool,
    pub include_license_record: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutedExport {
    pub source_path: String,
    pub destination_path: String,
    pub bytes_copied: u64,
    pub license_record_expected: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportQueueStatus {
    Ready,
    WaitingForSource,
    Completed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueuedExport {
    pub queue_id: u64,
    pub plan: ExportPlan,
    pub status: ExportQueueStatus,
    pub attempts: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportQueue {
    next_queue_id: u64,
    pub entries: Vec<QueuedExport>,
}

#[derive(Debug)]
pub enum ExportExecutionError {
    UnsupportedConversion,
    UnsupportedRange,
    Io(io::Error),
}

impl PartialEq for ExportExecutionError {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (
                ExportExecutionError::UnsupportedConversion,
                ExportExecutionError::UnsupportedConversion
            ) | (
                ExportExecutionError::UnsupportedRange,
                ExportExecutionError::UnsupportedRange
            )
        )
    }
}

impl Eq for ExportExecutionError {}

impl From<io::Error> for ExportExecutionError {
    fn from(error: io::Error) -> Self {
        ExportExecutionError::Io(error)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LicenseReportRow {
    pub asset_title: String,
    pub original_filename: String,
    pub provider: Option<String>,
    pub source_url: Option<String>,
    pub license_type: Option<String>,
    pub license_status: Option<String>,
    pub attribution: Option<String>,
    pub restrictions: Option<String>,
    pub receipt_path: Option<String>,
    pub usage_status: String,
    pub destination: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LicenseStatus {
    Active,
    Missing,
    Uncertain,
    Expired,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LicenseContext {
    pub provider: Option<String>,
    pub source_url: Option<String>,
    pub license_status: LicenseStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportLicenseWarning {
    MissingSource,
    MissingSourceUrl,
    MissingLicense,
    UncertainLicense,
    ExpiredLicense,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExportLicenseAssessment {
    Clear,
    Warn(Vec<ExportLicenseWarning>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExportPlanError {
    InvalidRange,
}

pub fn default_editorial_export_intent() -> ExportIntent {
    ExportIntent {
        preserve_original: true,
        include_license_record: true,
    }
}

pub fn plan_editorial_export(request: ExportRequest) -> Result<ExportPlan, ExportPlanError> {
    if let Some(range) = request.range {
        if range.start_ms >= range.end_ms {
            return Err(ExportPlanError::InvalidRange);
        }
    }

    let extension = match request.preset {
        ExportPreset::Original => request
            .source_path
            .rsplit_once('.')
            .map_or("wav", |(_, extension)| extension),
        ExportPreset::Wav48k24Bit => "wav",
    };
    let conversion = match request.preset {
        ExportPreset::Original => None,
        ExportPreset::Wav48k24Bit => Some(AudioConversion {
            sample_rate: 48_000,
            bit_depth: 24,
        }),
    };
    let filename = format!(
        "{}.{}",
        sanitize_filename(&request.asset_display_name),
        extension
    );

    Ok(ExportPlan {
        source_path: request.source_path,
        destination_path: format!(
            "{}/{}",
            request.project_media_dir.trim_end_matches('/'),
            filename
        ),
        range: request.range,
        conversion,
        preserve_original: request.intent.preserve_original,
        include_license_record: request.intent.include_license_record,
    })
}

pub fn assess_export_license(context: &LicenseContext) -> ExportLicenseAssessment {
    let mut warnings = Vec::new();

    if context
        .provider
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        warnings.push(ExportLicenseWarning::MissingSource);
    }

    if context
        .source_url
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
        && context.provider.is_some()
    {
        warnings.push(ExportLicenseWarning::MissingSourceUrl);
    }

    match context.license_status {
        LicenseStatus::Active => {}
        LicenseStatus::Missing => warnings.push(ExportLicenseWarning::MissingLicense),
        LicenseStatus::Uncertain => warnings.push(ExportLicenseWarning::UncertainLicense),
        LicenseStatus::Expired => warnings.push(ExportLicenseWarning::ExpiredLicense),
    }

    if warnings.is_empty() {
        ExportLicenseAssessment::Clear
    } else {
        ExportLicenseAssessment::Warn(warnings)
    }
}

pub fn execute_original_copy_export(
    plan: &ExportPlan,
) -> Result<ExecutedExport, ExportExecutionError> {
    if plan.conversion.is_some() {
        return Err(ExportExecutionError::UnsupportedConversion);
    }

    if plan.range.is_some() {
        return Err(ExportExecutionError::UnsupportedRange);
    }

    if let Some(parent) = Path::new(&plan.destination_path).parent() {
        fs::create_dir_all(parent)?;
    }

    let bytes_copied = fs::copy(&plan.source_path, &plan.destination_path)?;

    Ok(ExecutedExport {
        source_path: plan.source_path.clone(),
        destination_path: plan.destination_path.clone(),
        bytes_copied,
        license_record_expected: plan.include_license_record,
    })
}

impl ExportQueue {
    pub fn new() -> Self {
        Self {
            next_queue_id: 1,
            entries: Vec::new(),
        }
    }

    pub fn enqueue(
        &mut self,
        plan: ExportPlan,
        source_exists: impl Fn(&str) -> bool,
    ) -> QueuedExport {
        let status = if source_exists(&plan.source_path) {
            ExportQueueStatus::Ready
        } else {
            ExportQueueStatus::WaitingForSource
        };
        let queued = QueuedExport {
            queue_id: self.next_queue_id,
            plan,
            status,
            attempts: 0,
        };
        self.next_queue_id += 1;
        self.entries.push(queued.clone());

        queued
    }

    pub fn refresh_source_availability(&mut self, source_exists: impl Fn(&str) -> bool) {
        for entry in &mut self.entries {
            if entry.status == ExportQueueStatus::WaitingForSource
                && source_exists(&entry.plan.source_path)
            {
                entry.status = ExportQueueStatus::Ready;
            }
        }
    }

    pub fn next_ready_original_copy(&self) -> Option<&QueuedExport> {
        self.entries.iter().find(|entry| {
            entry.status == ExportQueueStatus::Ready
                && entry.plan.conversion.is_none()
                && entry.plan.range.is_none()
        })
    }

    pub fn mark_completed(&mut self, queue_id: u64) -> bool {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.queue_id == queue_id)
        {
            entry.status = ExportQueueStatus::Completed;
            return true;
        }

        false
    }
}

impl Default for ExportQueue {
    fn default() -> Self {
        Self::new()
    }
}

pub fn render_license_report_csv(rows: &[LicenseReportRow]) -> String {
    let mut output = String::from(
        "asset_title,original_filename,provider,source_url,license_type,license_status,attribution,restrictions,receipt_path,usage_status,destination\n",
    );

    for row in rows {
        let fields = [
            row.asset_title.as_str(),
            row.original_filename.as_str(),
            row.provider.as_deref().unwrap_or_default(),
            row.source_url.as_deref().unwrap_or_default(),
            row.license_type.as_deref().unwrap_or_default(),
            row.license_status.as_deref().unwrap_or_default(),
            row.attribution.as_deref().unwrap_or_default(),
            row.restrictions.as_deref().unwrap_or_default(),
            row.receipt_path.as_deref().unwrap_or_default(),
            row.usage_status.as_str(),
            row.destination.as_deref().unwrap_or_default(),
        ];
        output.push_str(
            &fields
                .iter()
                .map(|field| csv_escape(field))
                .collect::<Vec<_>>()
                .join(","),
        );
        output.push('\n');
    }

    output
}

impl ExportLicenseAssessment {
    pub fn allows_export(&self) -> bool {
        true
    }
}

fn sanitize_filename(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|character| {
            if matches!(character, '/' | ':' | '\\') {
                '-'
            } else {
                character
            }
        })
        .collect()
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn exports_preserve_traceability_by_default() {
        assert_eq!(
            default_editorial_export_intent(),
            ExportIntent {
                preserve_original: true,
                include_license_record: true,
            }
        );
    }

    #[test]
    fn copy_export_plan_preserves_original_and_traceability() {
        let plan = plan_editorial_export(ExportRequest {
            source_path: "/library/Media/00/impact.wav".to_string(),
            project_media_dir: "/projects/trailer/audio".to_string(),
            asset_display_name: "Dark Impact".to_string(),
            preset: ExportPreset::Original,
            range: None,
            intent: default_editorial_export_intent(),
        })
        .expect("plan");

        assert_eq!(plan.source_path, "/library/Media/00/impact.wav");
        assert_eq!(
            plan.destination_path,
            "/projects/trailer/audio/Dark Impact.wav"
        );
        assert!(plan.preserve_original);
        assert!(plan.include_license_record);
        assert_eq!(plan.conversion, None);
    }

    #[test]
    fn wav_preset_plans_conversion_without_mutating_library_original() {
        let plan = plan_editorial_export(ExportRequest {
            source_path: "/library/Media/00/music.mp3".to_string(),
            project_media_dir: "/projects/trailer/audio".to_string(),
            asset_display_name: "Theme".to_string(),
            preset: ExportPreset::Wav48k24Bit,
            range: Some(ExportRangeMs {
                start_ms: 1_000,
                end_ms: 4_000,
            }),
            intent: default_editorial_export_intent(),
        })
        .expect("plan");

        assert_eq!(plan.destination_path, "/projects/trailer/audio/Theme.wav");
        assert_eq!(
            plan.range,
            Some(ExportRangeMs {
                start_ms: 1_000,
                end_ms: 4_000
            })
        );
        assert_eq!(
            plan.conversion,
            Some(AudioConversion {
                sample_rate: 48_000,
                bit_depth: 24
            })
        );
        assert!(plan.preserve_original);
    }

    #[test]
    fn export_range_must_have_positive_duration() {
        let result = plan_editorial_export(ExportRequest {
            source_path: "/library/hit.wav".to_string(),
            project_media_dir: "/projects/audio".to_string(),
            asset_display_name: "Hit".to_string(),
            preset: ExportPreset::Original,
            range: Some(ExportRangeMs {
                start_ms: 4_000,
                end_ms: 4_000,
            }),
            intent: default_editorial_export_intent(),
        });

        assert_eq!(result, Err(ExportPlanError::InvalidRange));
    }

    #[test]
    fn active_license_does_not_warn_before_export() {
        let assessment = assess_export_license(&LicenseContext {
            provider: Some("Boom Library".to_string()),
            source_url: Some("https://example.com/sound".to_string()),
            license_status: LicenseStatus::Active,
        });

        assert_eq!(assessment, ExportLicenseAssessment::Clear);
    }

    #[test]
    fn missing_license_warns_without_blocking_export() {
        let assessment = assess_export_license(&LicenseContext {
            provider: None,
            source_url: None,
            license_status: LicenseStatus::Missing,
        });

        assert_eq!(
            assessment,
            ExportLicenseAssessment::Warn(vec![
                ExportLicenseWarning::MissingSource,
                ExportLicenseWarning::MissingLicense
            ])
        );
        assert!(assessment.allows_export());
    }

    #[test]
    fn uncertain_license_warns_without_blocking_export() {
        let assessment = assess_export_license(&LicenseContext {
            provider: Some("Unknown Pack".to_string()),
            source_url: None,
            license_status: LicenseStatus::Uncertain,
        });

        assert_eq!(
            assessment,
            ExportLicenseAssessment::Warn(vec![
                ExportLicenseWarning::MissingSourceUrl,
                ExportLicenseWarning::UncertainLicense
            ])
        );
        assert!(assessment.allows_export());
    }

    #[test]
    fn original_copy_execution_creates_destination_without_mutating_source() {
        let source_path = unique_export_path("source.wav");
        let destination_path = unique_export_path("project/audio/Dark Hit.wav");
        fs::create_dir_all(source_path.parent().expect("source parent")).expect("source dir");
        fs::write(&source_path, b"audio-bytes").expect("source file");

        let plan = ExportPlan {
            source_path: source_path.to_string_lossy().to_string(),
            destination_path: destination_path.to_string_lossy().to_string(),
            range: None,
            conversion: None,
            preserve_original: true,
            include_license_record: true,
        };

        let executed = execute_original_copy_export(&plan).expect("execute");

        assert_eq!(executed.bytes_copied, 11);
        assert!(executed.license_record_expected);
        assert_eq!(fs::read(&source_path).expect("source"), b"audio-bytes");
        assert_eq!(
            fs::read(&destination_path).expect("destination"),
            b"audio-bytes"
        );
    }

    #[test]
    fn copy_execution_rejects_plans_that_require_conversion() {
        let plan = ExportPlan {
            source_path: "/library/music.mp3".to_string(),
            destination_path: "/project/audio/music.wav".to_string(),
            range: None,
            conversion: Some(AudioConversion {
                sample_rate: 48_000,
                bit_depth: 24,
            }),
            preserve_original: true,
            include_license_record: true,
        };

        assert_eq!(
            execute_original_copy_export(&plan).map(|_| ()),
            Err(ExportExecutionError::UnsupportedConversion)
        );
    }

    #[test]
    fn license_report_csv_includes_receipts_and_escapes_fields() {
        let csv = render_license_report_csv(&[LicenseReportRow {
            asset_title: "Dark, Metallic Hit".to_string(),
            original_filename: "impact.wav".to_string(),
            provider: Some("Boom Library".to_string()),
            source_url: Some("https://example.com/sound".to_string()),
            license_type: Some("subscription".to_string()),
            license_status: Some("active".to_string()),
            attribution: Some("Artist \"A\"".to_string()),
            restrictions: Some("client project only".to_string()),
            receipt_path: Some("receipts/boom.pdf".to_string()),
            usage_status: "exported".to_string(),
            destination: Some("/project/audio/impact.wav".to_string()),
        }]);

        assert!(csv.starts_with("asset_title,original_filename,provider"));
        assert!(csv.contains("\"Dark, Metallic Hit\""));
        assert!(csv.contains("\"Artist \"\"A\"\"\""));
        assert!(csv.contains("receipts/boom.pdf"));
    }

    #[test]
    fn export_queue_waits_for_offline_source_then_promotes_when_available() {
        let plan = original_copy_plan("/Volumes/TrueNAS/SFX/hit.wav", "/project/audio/hit.wav");
        let mut queue = ExportQueue::new();

        let queued = queue.enqueue(plan.clone(), |_| false);

        assert_eq!(queued.status, ExportQueueStatus::WaitingForSource);
        assert_eq!(queue.next_ready_original_copy(), None);

        queue.refresh_source_availability(|path| path == "/Volumes/TrueNAS/SFX/hit.wav");

        assert_eq!(queue.entries[0].status, ExportQueueStatus::Ready);
        assert_eq!(
            queue
                .next_ready_original_copy()
                .map(|entry| entry.plan.clone()),
            Some(plan)
        );
    }

    #[test]
    fn export_queue_returns_ready_original_copies_in_insert_order() {
        let conversion_plan = ExportPlan {
            conversion: Some(AudioConversion {
                sample_rate: 48_000,
                bit_depth: 24,
            }),
            ..original_copy_plan("/library/music.mp3", "/project/music.wav")
        };
        let first_copy = original_copy_plan("/library/a.wav", "/project/a.wav");
        let second_copy = original_copy_plan("/library/b.wav", "/project/b.wav");
        let mut queue = ExportQueue::new();

        queue.enqueue(conversion_plan, |_| true);
        let first = queue.enqueue(first_copy.clone(), |_| true);
        queue.enqueue(second_copy, |_| true);

        assert_eq!(
            queue.next_ready_original_copy().map(|entry| entry.queue_id),
            Some(first.queue_id)
        );
        assert!(queue.mark_completed(first.queue_id));
        assert_eq!(
            queue
                .next_ready_original_copy()
                .map(|entry| entry.plan.clone()),
            Some(original_copy_plan("/library/b.wav", "/project/b.wav"))
        );
    }

    fn unique_export_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("darkwave-export-{}", uuid_like_suffix()));
        path.push(name);
        let _ = fs::remove_file(&path);
        path
    }

    fn uuid_like_suffix() -> String {
        format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        )
    }

    fn original_copy_plan(source_path: &str, destination_path: &str) -> ExportPlan {
        ExportPlan {
            source_path: source_path.to_string(),
            destination_path: destination_path.to_string(),
            range: None,
            conversion: None,
            preserve_original: true,
            include_license_record: true,
        }
    }
}
