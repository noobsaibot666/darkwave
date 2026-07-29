#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotionPolicy {
    Full,
    Reduced,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaterialPolicy {
    Translucent,
    Solid,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccessibilityProfile {
    pub reduced_motion: bool,
    pub reduced_transparency: bool,
    pub high_contrast: bool,
}

impl AccessibilityProfile {
    pub fn motion_policy(&self) -> MotionPolicy {
        if self.reduced_motion {
            MotionPolicy::Reduced
        } else {
            MotionPolicy::Full
        }
    }

    pub fn material_policy(&self) -> MaterialPolicy {
        if self.reduced_transparency || self.high_contrast {
            MaterialPolicy::Solid
        } else {
            MaterialPolicy::Translucent
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoverySession {
    pub previous_session_open: bool,
    pub last_library_path: Option<String>,
    pub autosave_revision: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoveryPrompt {
    None,
    RestoreLibrary {
        library_path: String,
        autosave_revision: u64,
    },
}

pub fn recovery_prompt(session: &RecoverySession) -> RecoveryPrompt {
    match (
        session.previous_session_open,
        &session.last_library_path,
        session.autosave_revision,
    ) {
        (true, Some(library_path), Some(autosave_revision)) => RecoveryPrompt::RestoreLibrary {
            library_path: library_path.clone(),
            autosave_revision,
        },
        _ => RecoveryPrompt::None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GateState {
    NotStarted,
    Planned,
    Passed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateChannel {
    Stable,
    Beta,
    Nightly,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateChannelConfig {
    pub channel: UpdateChannel,
    pub manifest_url: String,
    pub public_key_id: String,
}

impl UpdateChannelConfig {
    pub fn has_complete_metadata(&self) -> bool {
        self.manifest_url.starts_with("https://") && !self.public_key_id.trim().is_empty()
    }
}

pub fn update_system_gate(config: Option<&UpdateChannelConfig>) -> GateState {
    config
        .filter(|config| config.has_complete_metadata())
        .map(|_| GateState::Passed)
        .unwrap_or(GateState::Planned)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SigningNotarizationConfig {
    pub macos_developer_id: String,
    pub macos_team_id: String,
    pub windows_certificate_thumbprint: String,
}

impl SigningNotarizationConfig {
    pub fn has_complete_metadata(&self) -> bool {
        !self.macos_developer_id.trim().is_empty()
            && !self.macos_team_id.trim().is_empty()
            && !self.windows_certificate_thumbprint.trim().is_empty()
    }
}

pub fn signing_notarization_gate(config: Option<&SigningNotarizationConfig>) -> GateState {
    config
        .filter(|config| config.has_complete_metadata())
        .map(|_| GateState::Passed)
        .unwrap_or(GateState::Planned)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseBlocker {
    MacosAudit,
    WindowsAudit,
    AccessibilityAudit,
    PerformanceProfile,
    CrashRecovery,
    OnboardingDocs,
    CodecPackaging,
    CodecLicenseReview,
    UpdateSystem,
    SigningNotarization,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseCandidate {
    pub macos_audit: GateState,
    pub windows_audit: GateState,
    pub accessibility_audit: GateState,
    pub performance_profile: GateState,
    pub crash_recovery: GateState,
    pub onboarding_docs: GateState,
    pub codec_packaging: GateState,
    pub codec_license_review: GateState,
    pub update_system: GateState,
    pub signing_notarization: GateState,
}

impl ReleaseCandidate {
    pub fn blockers(&self) -> Vec<ReleaseBlocker> {
        let gates = [
            (ReleaseBlocker::MacosAudit, self.macos_audit),
            (ReleaseBlocker::WindowsAudit, self.windows_audit),
            (ReleaseBlocker::AccessibilityAudit, self.accessibility_audit),
            (ReleaseBlocker::PerformanceProfile, self.performance_profile),
            (ReleaseBlocker::CrashRecovery, self.crash_recovery),
            (ReleaseBlocker::OnboardingDocs, self.onboarding_docs),
            (ReleaseBlocker::CodecPackaging, self.codec_packaging),
            (
                ReleaseBlocker::CodecLicenseReview,
                self.codec_license_review,
            ),
            (ReleaseBlocker::UpdateSystem, self.update_system),
            (
                ReleaseBlocker::SigningNotarization,
                self.signing_notarization,
            ),
        ];

        gates
            .into_iter()
            .filter_map(|(blocker, state)| (state != GateState::Passed).then_some(blocker))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduced_motion_preference_disables_animated_surfaces() {
        let profile = AccessibilityProfile {
            reduced_motion: true,
            reduced_transparency: false,
            high_contrast: false,
        };

        assert_eq!(profile.motion_policy(), MotionPolicy::Reduced);
        assert_eq!(profile.material_policy(), MaterialPolicy::Translucent);
    }

    #[test]
    fn reduced_transparency_preference_uses_solid_materials() {
        let profile = AccessibilityProfile {
            reduced_motion: false,
            reduced_transparency: true,
            high_contrast: false,
        };

        assert_eq!(profile.motion_policy(), MotionPolicy::Full);
        assert_eq!(profile.material_policy(), MaterialPolicy::Solid);
    }

    #[test]
    fn crash_recovery_prompts_when_previous_session_did_not_close_cleanly() {
        let session = RecoverySession {
            previous_session_open: true,
            last_library_path: Some("/Volumes/Sound/Darkwave.darkwave".to_string()),
            autosave_revision: Some(42),
        };

        assert_eq!(
            recovery_prompt(&session),
            RecoveryPrompt::RestoreLibrary {
                library_path: "/Volumes/Sound/Darkwave.darkwave".to_string(),
                autosave_revision: 42,
            }
        );
    }

    #[test]
    fn release_candidate_requires_platform_audit_docs_and_signing_plan() {
        let candidate = ReleaseCandidate {
            macos_audit: GateState::Passed,
            windows_audit: GateState::Passed,
            accessibility_audit: GateState::Passed,
            performance_profile: GateState::Passed,
            crash_recovery: GateState::Passed,
            onboarding_docs: GateState::Passed,
            codec_packaging: GateState::Planned,
            codec_license_review: GateState::Planned,
            update_system: GateState::Planned,
            signing_notarization: GateState::Planned,
        };

        assert_eq!(
            candidate.blockers(),
            vec![
                ReleaseBlocker::CodecPackaging,
                ReleaseBlocker::CodecLicenseReview,
                ReleaseBlocker::UpdateSystem,
                ReleaseBlocker::SigningNotarization
            ]
        );
    }

    #[test]
    fn update_system_passes_with_https_manifest_and_public_key() {
        let config = UpdateChannelConfig {
            channel: UpdateChannel::Stable,
            manifest_url: "https://updates.darkwave.example/stable/latest.json".to_string(),
            public_key_id: "darkwave-release-2026".to_string(),
        };

        assert_eq!(update_system_gate(Some(&config)), GateState::Passed);
    }

    #[test]
    fn update_system_remains_planned_without_complete_channel_metadata() {
        assert_eq!(update_system_gate(None), GateState::Planned);

        let insecure_manifest = UpdateChannelConfig {
            channel: UpdateChannel::Beta,
            manifest_url: "http://updates.darkwave.example/beta/latest.json".to_string(),
            public_key_id: "darkwave-beta-2026".to_string(),
        };
        assert_eq!(
            update_system_gate(Some(&insecure_manifest)),
            GateState::Planned
        );

        let missing_key = UpdateChannelConfig {
            channel: UpdateChannel::Beta,
            manifest_url: "https://updates.darkwave.example/beta/latest.json".to_string(),
            public_key_id: " ".to_string(),
        };
        assert_eq!(update_system_gate(Some(&missing_key)), GateState::Planned);
    }

    #[test]
    fn signing_notarization_passes_with_platform_identity_metadata() {
        let config = SigningNotarizationConfig {
            macos_developer_id: "Developer ID Application: Darkwave Audio GmbH".to_string(),
            macos_team_id: "ABCD123456".to_string(),
            windows_certificate_thumbprint: "00112233445566778899AABBCCDDEEFF00112233".to_string(),
        };

        assert_eq!(signing_notarization_gate(Some(&config)), GateState::Passed);
    }

    #[test]
    fn signing_notarization_remains_planned_without_complete_identity_metadata() {
        assert_eq!(signing_notarization_gate(None), GateState::Planned);

        let missing_windows_certificate = SigningNotarizationConfig {
            macos_developer_id: "Developer ID Application: Darkwave Audio GmbH".to_string(),
            macos_team_id: "ABCD123456".to_string(),
            windows_certificate_thumbprint: " ".to_string(),
        };

        assert_eq!(
            signing_notarization_gate(Some(&missing_windows_certificate)),
            GateState::Planned
        );
    }
}
