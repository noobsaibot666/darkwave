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

#[cfg(test)]
mod tests {
    use super::*;

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
}
