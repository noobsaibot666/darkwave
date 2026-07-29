#[tauri::command]
fn healthcheck() -> &'static str {
    library_core::product_codename()
}

#[tauri::command]
fn release_blockers() -> Vec<&'static str> {
    use release_readiness::{GateState, ReleaseBlocker, ReleaseCandidate};

    ReleaseCandidate {
        macos_audit: GateState::Passed,
        windows_audit: GateState::Passed,
        accessibility_audit: GateState::Passed,
        performance_profile: GateState::Passed,
        crash_recovery: GateState::Passed,
        onboarding_docs: GateState::Passed,
        update_system: GateState::Planned,
        signing_notarization: GateState::Planned,
    }
    .blockers()
    .into_iter()
    .map(|blocker| match blocker {
        ReleaseBlocker::MacosAudit => "macos_audit",
        ReleaseBlocker::WindowsAudit => "windows_audit",
        ReleaseBlocker::AccessibilityAudit => "accessibility_audit",
        ReleaseBlocker::PerformanceProfile => "performance_profile",
        ReleaseBlocker::CrashRecovery => "crash_recovery",
        ReleaseBlocker::OnboardingDocs => "onboarding_docs",
        ReleaseBlocker::UpdateSystem => "update_system",
        ReleaseBlocker::SigningNotarization => "signing_notarization",
    })
    .collect()
}

#[tauri::command]
fn default_preferences() -> preferences::AppPreferences {
    preferences::AppPreferences::default_for_editorial_audio()
}

#[tauri::command]
fn supported_drag_targets() -> Vec<&'static str> {
    vec![
        "tag",
        "collection",
        "project",
        "favorite",
        "trash",
        "external_export",
    ]
}

#[tauri::command]
fn default_virtualized_range() -> (usize, usize) {
    let range = viewport::VirtualViewport {
        total_rows: 50_000,
        row_height_px: 52,
        viewport_height_px: 520,
        scroll_top_px: 0,
        overscan_rows: 6,
    }
    .visible_range();

    (range.start, range.end_exclusive)
}

#[tauri::command]
fn default_command_titles() -> Vec<String> {
    command_palette::CommandRegistry::default_audio_workspace()
        .search("")
        .into_iter()
        .map(|command| command.title)
        .collect()
}

#[tauri::command]
fn sample_maintenance_summary() -> (usize, &'static str) {
    let report = maintenance::MaintenanceReport::from_findings(Vec::new());
    let severity = match report.severity {
        maintenance::MaintenanceSeverity::Ok => "ok",
        maintenance::MaintenanceSeverity::Warning => "warning",
    };

    (report.total_findings, severity)
}

#[tauri::command]
fn trash_retention_policy_days() -> u64 {
    30
}

#[tauri::command]
fn backup_restore_requirements() -> Vec<&'static str> {
    vec!["catalog_snapshot", "portable_manifest", "media_root"]
}

#[tauri::command]
fn sample_media_root_status() -> (&'static str, bool) {
    let probe = library_sync::probe_media_root("/Volumes/TrueNAS/SFX", |_| false);
    let status = match probe.status {
        library_sync::MediaRootStatus::Online => "online",
        library_sync::MediaRootStatus::Offline => "offline",
    };

    (status, probe.reconnect_validation_required)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            healthcheck,
            release_blockers,
            default_preferences,
            supported_drag_targets,
            default_virtualized_range,
            default_command_titles,
            sample_maintenance_summary,
            trash_retention_policy_days,
            backup_restore_requirements,
            sample_media_root_status
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Darkwave desktop shell");
}

#[cfg(test)]
mod tests {
    #[test]
    fn healthcheck_returns_product_codename() {
        assert_eq!(super::healthcheck(), "Darkwave");
    }

    #[test]
    fn release_blockers_expose_planned_distribution_work() {
        assert_eq!(
            super::release_blockers(),
            vec!["update_system", "signing_notarization"]
        );
    }

    #[test]
    fn default_preferences_expose_audio_workspace_shortcuts() {
        let preferences = super::default_preferences();

        assert_eq!(
            preferences
                .shortcuts
                .binding_for(preferences::CommandId::TogglePlayback),
            Some("Space")
        );
    }

    #[test]
    fn supported_drag_targets_include_classification_and_export() {
        assert_eq!(
            super::supported_drag_targets(),
            vec![
                "tag",
                "collection",
                "project",
                "favorite",
                "trash",
                "external_export"
            ]
        );
    }

    #[test]
    fn default_virtualized_range_keeps_initial_render_bounded() {
        assert_eq!(super::default_virtualized_range(), (0, 16));
    }

    #[test]
    fn default_command_titles_include_import_and_search_first() {
        assert_eq!(
            &super::default_command_titles()[0..2],
            ["Import Folder".to_string(), "Focus Search".to_string()]
        );
    }

    #[test]
    fn sample_maintenance_summary_reports_clean_state() {
        assert_eq!(super::sample_maintenance_summary(), (0, "ok"));
    }

    #[test]
    fn trash_retention_policy_defaults_to_30_days() {
        assert_eq!(super::trash_retention_policy_days(), 30);
    }

    #[test]
    fn backup_restore_requirements_include_catalog_manifest_and_media_root() {
        assert_eq!(
            super::backup_restore_requirements(),
            vec!["catalog_snapshot", "portable_manifest", "media_root"]
        );
    }

    #[test]
    fn sample_media_root_status_reports_offline_without_reconnect_validation() {
        assert_eq!(super::sample_media_root_status(), ("offline", false));
    }
}
