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

#[derive(Clone, Debug, PartialEq)]
pub struct DecodedPcmBuffer {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedWavExport {
    pub destination_path: String,
    pub frames_rendered: usize,
    pub bytes_written: u64,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalDragOperation {
    Copy,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalDragPayload {
    pub file_urls: Vec<String>,
    pub operation: ExternalDragOperation,
    pub include_license_report: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExternalDragPayloadError {
    EmptySelection,
    RequiresRenderedExport,
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
    UnsupportedBitDepth(u16),
    DecodedSampleRateMismatch { expected: u32, actual: u32 },
    InvalidDecodedAudio,
    Io(io::Error),
}

impl PartialEq for ExportExecutionError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                ExportExecutionError::UnsupportedConversion,
                ExportExecutionError::UnsupportedConversion,
            )
            | (ExportExecutionError::UnsupportedRange, ExportExecutionError::UnsupportedRange)
            | (
                ExportExecutionError::InvalidDecodedAudio,
                ExportExecutionError::InvalidDecodedAudio,
            ) => true,
            (
                ExportExecutionError::UnsupportedBitDepth(left),
                ExportExecutionError::UnsupportedBitDepth(right),
            ) => left == right,
            (
                ExportExecutionError::DecodedSampleRateMismatch {
                    expected: left_expected,
                    actual: left_actual,
                },
                ExportExecutionError::DecodedSampleRateMismatch {
                    expected: right_expected,
                    actual: right_actual,
                },
            ) => left_expected == right_expected && left_actual == right_actual,
            _ => false,
        }
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

pub fn build_external_drag_payload(
    plans: &[ExportPlan],
) -> Result<ExternalDragPayload, ExternalDragPayloadError> {
    if plans.is_empty() {
        return Err(ExternalDragPayloadError::EmptySelection);
    }

    if plans
        .iter()
        .any(|plan| plan.conversion.is_some() || plan.range.is_some())
    {
        return Err(ExternalDragPayloadError::RequiresRenderedExport);
    }

    Ok(ExternalDragPayload {
        file_urls: plans
            .iter()
            .map(|plan| file_url_for_path(&plan.destination_path))
            .collect(),
        operation: ExternalDragOperation::Copy,
        include_license_report: plans.iter().any(|plan| plan.include_license_record),
    })
}

pub fn build_rendered_external_drag_payload(
    rendered_exports: &[RenderedWavExport],
) -> Result<ExternalDragPayload, ExternalDragPayloadError> {
    if rendered_exports.is_empty() {
        return Err(ExternalDragPayloadError::EmptySelection);
    }

    Ok(ExternalDragPayload {
        file_urls: rendered_exports
            .iter()
            .map(|rendered| file_url_for_path(&rendered.destination_path))
            .collect(),
        operation: ExternalDragOperation::Copy,
        include_license_report: rendered_exports
            .iter()
            .any(|rendered| rendered.license_record_expected),
    })
}

pub fn render_wav_export(
    plan: &ExportPlan,
    decoded: &DecodedPcmBuffer,
) -> Result<RenderedWavExport, ExportExecutionError> {
    let conversion = plan
        .conversion
        .ok_or(ExportExecutionError::UnsupportedConversion)?;
    if conversion.bit_depth != 24 {
        return Err(ExportExecutionError::UnsupportedBitDepth(
            conversion.bit_depth,
        ));
    }
    if decoded.sample_rate != conversion.sample_rate {
        return Err(ExportExecutionError::DecodedSampleRateMismatch {
            expected: conversion.sample_rate,
            actual: decoded.sample_rate,
        });
    }
    if decoded.channels == 0 || decoded.samples.len() % decoded.channels as usize != 0 {
        return Err(ExportExecutionError::InvalidDecodedAudio);
    }

    let channel_count = decoded.channels as usize;
    let total_frames = decoded.samples.len() / channel_count;
    let (start_frame, end_frame) = render_frame_range(plan.range, conversion.sample_rate);
    let start_frame = start_frame.min(total_frames);
    let end_frame = end_frame.min(total_frames);
    if start_frame >= end_frame {
        return Err(ExportExecutionError::UnsupportedRange);
    }

    let sample_start = start_frame * channel_count;
    let sample_end = end_frame * channel_count;
    let wav = encode_wav_24_bit_pcm(
        &decoded.samples[sample_start..sample_end],
        conversion.sample_rate,
        decoded.channels,
    );

    if let Some(parent) = Path::new(&plan.destination_path).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&plan.destination_path, &wav)?;

    Ok(RenderedWavExport {
        destination_path: plan.destination_path.clone(),
        frames_rendered: end_frame - start_frame,
        bytes_written: wav.len() as u64,
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

fn file_url_for_path(path: &str) -> String {
    let encoded_path = percent_encode_path(path);
    if path.starts_with('/') {
        format!("file://{}", encoded_path)
    } else {
        format!("file:///{}", encoded_path)
    }
}

fn percent_encode_path(path: &str) -> String {
    let mut encoded = String::new();

    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'-' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }

    encoded
}

fn render_frame_range(range: Option<ExportRangeMs>, sample_rate: u32) -> (usize, usize) {
    range
        .map(|range| {
            (
                ms_to_frame(range.start_ms, sample_rate),
                ms_to_frame(range.end_ms, sample_rate),
            )
        })
        .unwrap_or((0, usize::MAX))
}

fn ms_to_frame(ms: u64, sample_rate: u32) -> usize {
    ((ms as u128 * sample_rate as u128) / 1_000) as usize
}

fn encode_wav_24_bit_pcm(samples: &[f32], sample_rate: u32, channels: u16) -> Vec<u8> {
    let bytes_per_sample = 3u16;
    let data_size = samples.len() as u32 * bytes_per_sample as u32;
    let byte_rate = sample_rate * channels as u32 * bytes_per_sample as u32;
    let block_align = channels * bytes_per_sample;
    let mut wav = Vec::with_capacity(44 + data_size as usize);

    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&24u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());

    for sample in samples {
        let scaled = (sample.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32;
        let bytes = scaled.to_le_bytes();
        wav.extend_from_slice(&bytes[0..3]);
    }

    wav
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

    #[test]
    fn external_drag_payload_uses_ready_destination_file_urls_in_order() {
        let first = original_copy_plan("/library/a.wav", "/project/audio/Dark Hit.wav");
        let second = ExportPlan {
            include_license_record: false,
            ..original_copy_plan("/library/b.wav", "/project/audio/b.wav")
        };

        let payload = build_external_drag_payload(&[first, second]).expect("external drag payload");

        assert_eq!(
            payload,
            ExternalDragPayload {
                file_urls: vec![
                    "file:///project/audio/Dark%20Hit.wav".to_string(),
                    "file:///project/audio/b.wav".to_string()
                ],
                operation: ExternalDragOperation::Copy,
                include_license_report: true
            }
        );
    }

    #[test]
    fn external_drag_payload_rejects_exports_that_still_need_rendering() {
        let conversion_plan = ExportPlan {
            conversion: Some(AudioConversion {
                sample_rate: 48_000,
                bit_depth: 24,
            }),
            ..original_copy_plan("/library/music.mp3", "/project/audio/music.wav")
        };
        let ranged_plan = ExportPlan {
            range: Some(ExportRangeMs {
                start_ms: 100,
                end_ms: 400,
            }),
            ..original_copy_plan("/library/impact.wav", "/project/audio/impact.wav")
        };

        assert_eq!(
            build_external_drag_payload(&[conversion_plan]),
            Err(ExternalDragPayloadError::RequiresRenderedExport)
        );
        assert_eq!(
            build_external_drag_payload(&[ranged_plan]),
            Err(ExternalDragPayloadError::RequiresRenderedExport)
        );
    }

    #[test]
    fn wav_render_export_writes_24_bit_pcm_with_selected_range() {
        let destination_path = unique_export_path("project/audio/range.wav");
        let plan = ExportPlan {
            source_path: "/library/source.wav".to_string(),
            destination_path: destination_path.to_string_lossy().to_string(),
            range: Some(ExportRangeMs {
                start_ms: 1,
                end_ms: 3,
            }),
            conversion: Some(AudioConversion {
                sample_rate: 1_000,
                bit_depth: 24,
            }),
            preserve_original: true,
            include_license_record: true,
        };
        let decoded = DecodedPcmBuffer {
            sample_rate: 1_000,
            channels: 1,
            samples: vec![-1.0, -0.5, 0.0, 0.5],
        };

        let rendered = render_wav_export(&plan, &decoded).expect("render wav");
        let wav = fs::read(&destination_path).expect("wav");

        assert_eq!(rendered.frames_rendered, 2);
        assert_eq!(rendered.bytes_written, wav.len() as u64);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 1);
        assert_eq!(
            u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]),
            1_000
        );
        assert_eq!(u16::from_le_bytes([wav[34], wav[35]]), 24);
        assert_eq!(u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]), 6);
        assert_eq!(wav.len(), 44 + 6);
    }

    #[test]
    fn wav_render_export_rejects_mismatched_decoded_sample_rate() {
        let plan = ExportPlan {
            source_path: "/library/source.mp3".to_string(),
            destination_path: "/project/audio/source.wav".to_string(),
            range: None,
            conversion: Some(AudioConversion {
                sample_rate: 48_000,
                bit_depth: 24,
            }),
            preserve_original: true,
            include_license_record: true,
        };
        let decoded = DecodedPcmBuffer {
            sample_rate: 44_100,
            channels: 2,
            samples: vec![0.0, 0.0],
        };

        assert_eq!(
            render_wav_export(&plan, &decoded).map(|_| ()),
            Err(ExportExecutionError::DecodedSampleRateMismatch {
                expected: 48_000,
                actual: 44_100,
            })
        );
    }

    #[test]
    fn rendered_exports_build_external_drag_payload_after_wav_rendering() {
        let rendered = RenderedWavExport {
            destination_path: "/project/audio/Dark Hit.wav".to_string(),
            frames_rendered: 480,
            bytes_written: 1_484,
            license_record_expected: true,
        };

        let payload =
            build_rendered_external_drag_payload(&[rendered]).expect("rendered drag payload");

        assert_eq!(
            payload,
            ExternalDragPayload {
                file_urls: vec!["file:///project/audio/Dark%20Hit.wav".to_string()],
                operation: ExternalDragOperation::Copy,
                include_license_report: true,
            }
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
