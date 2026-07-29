use audio_metadata::{decode_wav_pcm, DecodedAudioBuffer, MetadataError};
use preferences::OutputDevicePreference;
use shared_types::AvailabilityState;
use uuid::Uuid;

pub const NORMAL_PLAYBACK_SPEED_PERCENT: u16 = 100;
pub const MIN_PLAYBACK_SPEED_PERCENT: u16 = 50;
pub const MAX_PLAYBACK_SPEED_PERCENT: u16 = 200;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackCommand {
    Play,
    Pause,
    Stop,
    SeekMs(u64),
}

pub fn command_priority(command: PlaybackCommand) -> u8 {
    match command {
        PlaybackCommand::Play | PlaybackCommand::SeekMs(_) => 0,
        PlaybackCommand::Pause => 1,
        PlaybackCommand::Stop => 2,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoopRegion {
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackEvent {
    Load { asset_id: Uuid, duration_ms: u64 },
    Play,
    Pause,
    Stop,
    SeekMs(u64),
    SetLoopRegion { start_ms: u64, end_ms: u64 },
    ClearLoopRegion,
    SetPlaybackSpeedPercent(u16),
    ResetPlaybackSpeed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackSession {
    active_asset_id: Option<Uuid>,
    duration_ms: u64,
    position_ms: u64,
    playing: bool,
    loop_region: Option<LoopRegion>,
    playback_speed_percent: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackSourceRequest {
    pub asset_id: Uuid,
    pub original_path: String,
    pub availability_state: AvailabilityState,
    pub cached_preview_path: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlaybackSource {
    Original { asset_id: Uuid, path: String },
    CachedPreview { asset_id: Uuid, path: String },
    Unavailable { asset_id: Uuid },
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreparedPreviewPlayback {
    pub asset_id: Uuid,
    pub decoded: DecodedAudioBuffer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaybackDecodeToken {
    pub asset_id: Uuid,
    pub generation: u64,
    pub requested_at_ms: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PlaybackDecodeCoordinator {
    active_token: Option<PlaybackDecodeToken>,
    next_generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaybackStartupMeasurement {
    pub requested_at_ms: u64,
    pub started_at_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackStartupLatency {
    Passed { elapsed_ms: u64 },
    TooSlow { elapsed_ms: u64 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioOutputRouteRequest {
    pub preference: OutputDevicePreference,
    pub available_device_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AudioOutputRoute {
    SystemDefault,
    Device { device_id: String },
    FallbackToSystemDefault { missing_device_id: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformAudioOutput {
    pub device_id: String,
    pub handle_id: String,
    pub is_system_default: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioOutputBindingRequest {
    pub route: AudioOutputRoute,
    pub platform_outputs: Vec<PlatformAudioOutput>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioOutputBindingFallback {
    NoSystemDefaultHandle,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AudioOutputBinding {
    Bound {
        route: AudioOutputRoute,
        handle_id: String,
    },
    UnboundSystemDefault {
        reason: AudioOutputBindingFallback,
    },
}

pub fn choose_playback_source(request: PlaybackSourceRequest) -> PlaybackSource {
    match request.availability_state {
        AvailabilityState::Local => PlaybackSource::Original {
            asset_id: request.asset_id,
            path: request.original_path,
        },
        AvailabilityState::Cached | AvailabilityState::Missing | AvailabilityState::Unknown => {
            request
                .cached_preview_path
                .map(|path| PlaybackSource::CachedPreview {
                    asset_id: request.asset_id,
                    path,
                })
                .unwrap_or(PlaybackSource::Unavailable {
                    asset_id: request.asset_id,
                })
        }
    }
}

pub fn choose_audio_output_route(request: AudioOutputRouteRequest) -> AudioOutputRoute {
    match request.preference {
        OutputDevicePreference::SystemDefault => AudioOutputRoute::SystemDefault,
        OutputDevicePreference::DeviceId(device_id) => {
            if request
                .available_device_ids
                .iter()
                .any(|available| available == &device_id)
            {
                AudioOutputRoute::Device { device_id }
            } else {
                AudioOutputRoute::FallbackToSystemDefault {
                    missing_device_id: device_id,
                }
            }
        }
    }
}

pub fn bind_audio_output_route(request: AudioOutputBindingRequest) -> AudioOutputBinding {
    let default_handle = || {
        request
            .platform_outputs
            .iter()
            .find(|output| output.is_system_default)
            .map(|output| output.handle_id.clone())
    };

    match &request.route {
        AudioOutputRoute::SystemDefault | AudioOutputRoute::FallbackToSystemDefault { .. } => {
            default_handle()
                .map(|handle_id| AudioOutputBinding::Bound {
                    route: request.route,
                    handle_id,
                })
                .unwrap_or(AudioOutputBinding::UnboundSystemDefault {
                    reason: AudioOutputBindingFallback::NoSystemDefaultHandle,
                })
        }
        AudioOutputRoute::Device { device_id } => request
            .platform_outputs
            .iter()
            .find(|output| output.device_id == *device_id)
            .map(|output| AudioOutputBinding::Bound {
                route: request.route.clone(),
                handle_id: output.handle_id.clone(),
            })
            .or_else(|| {
                default_handle().map(|handle_id| AudioOutputBinding::Bound {
                    route: AudioOutputRoute::FallbackToSystemDefault {
                        missing_device_id: device_id.clone(),
                    },
                    handle_id,
                })
            })
            .unwrap_or(AudioOutputBinding::UnboundSystemDefault {
                reason: AudioOutputBindingFallback::NoSystemDefaultHandle,
            }),
    }
}

pub fn prepare_cached_preview_playback(
    source: &PlaybackSource,
) -> Result<Option<PreparedPreviewPlayback>, MetadataError> {
    match source {
        PlaybackSource::CachedPreview { asset_id, path } => decode_wav_pcm(path).map(|decoded| {
            Some(PreparedPreviewPlayback {
                asset_id: *asset_id,
                decoded,
            })
        }),
        PlaybackSource::Original { .. } | PlaybackSource::Unavailable { .. } => Ok(None),
    }
}

pub fn classify_playback_startup_latency(
    measurement: PlaybackStartupMeasurement,
) -> PlaybackStartupLatency {
    let elapsed_ms = measurement
        .started_at_ms
        .saturating_sub(measurement.requested_at_ms);
    if elapsed_ms <= 100 {
        PlaybackStartupLatency::Passed { elapsed_ms }
    } else {
        PlaybackStartupLatency::TooSlow { elapsed_ms }
    }
}

fn clamp_playback_speed_percent(percent: u16) -> u16 {
    percent.clamp(MIN_PLAYBACK_SPEED_PERCENT, MAX_PLAYBACK_SPEED_PERCENT)
}

impl PlaybackDecodeCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin_decode(&mut self, asset_id: Uuid, requested_at_ms: u64) -> PlaybackDecodeToken {
        self.next_generation += 1;
        let token = PlaybackDecodeToken {
            asset_id,
            generation: self.next_generation,
            requested_at_ms,
        };
        self.active_token = Some(token);
        token
    }

    pub fn is_cancelled(&self, token: &PlaybackDecodeToken) -> bool {
        self.active_token.as_ref() != Some(token)
    }

    pub fn active_asset_id(&self) -> Option<Uuid> {
        self.active_token.map(|token| token.asset_id)
    }
}

impl Default for PlaybackSession {
    fn default() -> Self {
        Self {
            active_asset_id: None,
            duration_ms: 0,
            position_ms: 0,
            playing: false,
            loop_region: None,
            playback_speed_percent: NORMAL_PLAYBACK_SPEED_PERCENT,
        }
    }
}

impl PlaybackSession {
    pub fn apply(&mut self, event: PlaybackEvent) {
        match event {
            PlaybackEvent::Load {
                asset_id,
                duration_ms,
            } => {
                self.active_asset_id = Some(asset_id);
                self.duration_ms = duration_ms;
                self.position_ms = 0;
                self.playing = false;
                self.loop_region = None;
            }
            PlaybackEvent::Play => {
                if self.active_asset_id.is_some() {
                    self.playing = true;
                }
            }
            PlaybackEvent::Pause => {
                self.playing = false;
            }
            PlaybackEvent::Stop => {
                self.playing = false;
                self.position_ms = 0;
            }
            PlaybackEvent::SeekMs(position_ms) => {
                self.position_ms = position_ms.min(self.duration_ms);
            }
            PlaybackEvent::SetLoopRegion { start_ms, end_ms } => {
                let start_ms = start_ms.min(self.duration_ms);
                let end_ms = end_ms.min(self.duration_ms);
                self.loop_region = (start_ms < end_ms).then_some(LoopRegion { start_ms, end_ms });
            }
            PlaybackEvent::ClearLoopRegion => {
                self.loop_region = None;
            }
            PlaybackEvent::SetPlaybackSpeedPercent(percent) => {
                self.playback_speed_percent = clamp_playback_speed_percent(percent);
            }
            PlaybackEvent::ResetPlaybackSpeed => {
                self.playback_speed_percent = NORMAL_PLAYBACK_SPEED_PERCENT;
            }
        }
    }

    pub fn active_asset_id(&self) -> Option<Uuid> {
        self.active_asset_id
    }

    pub fn position_ms(&self) -> u64 {
        self.position_ms
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn loop_region(&self) -> Option<LoopRegion> {
        self.loop_region
    }

    pub fn playback_speed_percent(&self) -> u16 {
        self.playback_speed_percent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use preferences::OutputDevicePreference;
    use shared_types::AvailabilityState;
    use uuid::Uuid;

    #[test]
    fn playback_commands_outrank_background_work() {
        assert_eq!(command_priority(PlaybackCommand::Play), 0);
    }

    #[test]
    fn selecting_next_asset_cancels_previous_playback() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let mut session = PlaybackSession::default();

        session.apply(PlaybackEvent::Load {
            asset_id: first,
            duration_ms: 2_000,
        });
        session.apply(PlaybackEvent::Play);
        session.apply(PlaybackEvent::Load {
            asset_id: second,
            duration_ms: 4_000,
        });

        assert_eq!(session.active_asset_id(), Some(second));
        assert_eq!(session.position_ms(), 0);
        assert!(!session.is_playing());
    }

    #[test]
    fn seek_clamps_to_loaded_asset_duration() {
        let mut session = PlaybackSession::default();
        session.apply(PlaybackEvent::Load {
            asset_id: Uuid::new_v4(),
            duration_ms: 1_500,
        });
        session.apply(PlaybackEvent::SeekMs(9_000));

        assert_eq!(session.position_ms(), 1_500);
    }

    #[test]
    fn loop_region_is_kept_when_playback_starts() {
        let mut session = PlaybackSession::default();
        session.apply(PlaybackEvent::Load {
            asset_id: Uuid::new_v4(),
            duration_ms: 10_000,
        });
        session.apply(PlaybackEvent::SetLoopRegion {
            start_ms: 400,
            end_ms: 1_200,
        });
        session.apply(PlaybackEvent::Play);

        assert_eq!(
            session.loop_region(),
            Some(LoopRegion {
                start_ms: 400,
                end_ms: 1_200
            })
        );
        assert!(session.is_playing());
    }

    #[test]
    fn playback_speed_defaults_to_normal_and_clamps_to_editorial_range() {
        let mut session = PlaybackSession::default();

        assert_eq!(session.playback_speed_percent(), 100);

        session.apply(PlaybackEvent::SetPlaybackSpeedPercent(25));
        assert_eq!(session.playback_speed_percent(), 50);

        session.apply(PlaybackEvent::SetPlaybackSpeedPercent(240));
        assert_eq!(session.playback_speed_percent(), 200);
    }

    #[test]
    fn playback_speed_reset_returns_to_normal_without_reloading_asset() {
        let mut session = PlaybackSession::default();
        let asset_id = Uuid::new_v4();
        session.apply(PlaybackEvent::Load {
            asset_id,
            duration_ms: 10_000,
        });
        session.apply(PlaybackEvent::Play);
        session.apply(PlaybackEvent::SetPlaybackSpeedPercent(75));

        session.apply(PlaybackEvent::ResetPlaybackSpeed);

        assert_eq!(session.active_asset_id(), Some(asset_id));
        assert!(session.is_playing());
        assert_eq!(session.playback_speed_percent(), 100);
    }

    #[test]
    fn playback_source_uses_original_when_asset_is_local() {
        let asset_id = Uuid::new_v4();

        let source = choose_playback_source(PlaybackSourceRequest {
            asset_id,
            original_path: "/library/Media/00/hit.wav".to_string(),
            availability_state: AvailabilityState::Local,
            cached_preview_path: Some("/cache/previews/hit.ogg".to_string()),
        });

        assert_eq!(
            source,
            PlaybackSource::Original {
                asset_id,
                path: "/library/Media/00/hit.wav".to_string()
            }
        );
    }

    #[test]
    fn playback_source_uses_preview_cache_when_original_is_missing() {
        let asset_id = Uuid::new_v4();

        let source = choose_playback_source(PlaybackSourceRequest {
            asset_id,
            original_path: "/Volumes/TrueNAS/SFX/hit.wav".to_string(),
            availability_state: AvailabilityState::Missing,
            cached_preview_path: Some("/cache/previews/hit.ogg".to_string()),
        });

        assert_eq!(
            source,
            PlaybackSource::CachedPreview {
                asset_id,
                path: "/cache/previews/hit.ogg".to_string()
            }
        );
    }

    #[test]
    fn playback_source_reports_unavailable_without_original_or_cache() {
        let asset_id = Uuid::new_v4();

        let source = choose_playback_source(PlaybackSourceRequest {
            asset_id,
            original_path: "/Volumes/TrueNAS/SFX/hit.wav".to_string(),
            availability_state: AvailabilityState::Missing,
            cached_preview_path: None,
        });

        assert_eq!(source, PlaybackSource::Unavailable { asset_id });
    }

    #[test]
    fn audio_output_route_uses_system_default_preference() {
        let route = choose_audio_output_route(AudioOutputRouteRequest {
            preference: OutputDevicePreference::SystemDefault,
            available_device_ids: vec!["interface-a".to_string()],
        });

        assert_eq!(route, AudioOutputRoute::SystemDefault);
    }

    #[test]
    fn audio_output_route_uses_available_saved_device() {
        let route = choose_audio_output_route(AudioOutputRouteRequest {
            preference: OutputDevicePreference::DeviceId("interface-a".to_string()),
            available_device_ids: vec!["interface-a".to_string(), "speakers".to_string()],
        });

        assert_eq!(
            route,
            AudioOutputRoute::Device {
                device_id: "interface-a".to_string()
            }
        );
    }

    #[test]
    fn audio_output_route_falls_back_when_saved_device_is_missing() {
        let route = choose_audio_output_route(AudioOutputRouteRequest {
            preference: OutputDevicePreference::DeviceId("disconnected-interface".to_string()),
            available_device_ids: vec!["speakers".to_string()],
        });

        assert_eq!(
            route,
            AudioOutputRoute::FallbackToSystemDefault {
                missing_device_id: "disconnected-interface".to_string()
            }
        );
    }

    #[test]
    fn audio_output_binding_uses_platform_handle_for_selected_device() {
        let binding = bind_audio_output_route(AudioOutputBindingRequest {
            route: AudioOutputRoute::Device {
                device_id: "interface-a".to_string(),
            },
            platform_outputs: vec![
                PlatformAudioOutput {
                    device_id: "speakers".to_string(),
                    handle_id: "coreaudio-default".to_string(),
                    is_system_default: true,
                },
                PlatformAudioOutput {
                    device_id: "interface-a".to_string(),
                    handle_id: "coreaudio-interface-a".to_string(),
                    is_system_default: false,
                },
            ],
        });

        assert_eq!(
            binding,
            AudioOutputBinding::Bound {
                route: AudioOutputRoute::Device {
                    device_id: "interface-a".to_string()
                },
                handle_id: "coreaudio-interface-a".to_string()
            }
        );
    }

    #[test]
    fn audio_output_binding_uses_system_default_handle_for_default_route() {
        let binding = bind_audio_output_route(AudioOutputBindingRequest {
            route: AudioOutputRoute::SystemDefault,
            platform_outputs: vec![PlatformAudioOutput {
                device_id: "speakers".to_string(),
                handle_id: "coreaudio-default".to_string(),
                is_system_default: true,
            }],
        });

        assert_eq!(
            binding,
            AudioOutputBinding::Bound {
                route: AudioOutputRoute::SystemDefault,
                handle_id: "coreaudio-default".to_string()
            }
        );
    }

    #[test]
    fn audio_output_binding_records_unbound_fallback_without_platform_handle() {
        let binding = bind_audio_output_route(AudioOutputBindingRequest {
            route: AudioOutputRoute::FallbackToSystemDefault {
                missing_device_id: "interface-a".to_string(),
            },
            platform_outputs: Vec::new(),
        });

        assert_eq!(
            binding,
            AudioOutputBinding::UnboundSystemDefault {
                reason: AudioOutputBindingFallback::NoSystemDefaultHandle
            }
        );
    }

    #[test]
    fn cached_preview_playback_decodes_wav_preview() {
        let asset_id = Uuid::new_v4();
        let path = unique_wav_path();
        std::fs::write(&path, wav_16_bit_fixture()).expect("fixture");
        let source = PlaybackSource::CachedPreview {
            asset_id,
            path: path.to_string_lossy().to_string(),
        };

        let prepared = prepare_cached_preview_playback(&source)
            .expect("prepare")
            .expect("decoded preview");

        assert_eq!(prepared.asset_id, asset_id);
        assert_eq!(prepared.decoded.sample_rate, 48_000);
        assert_eq!(prepared.decoded.channels, 1);
        assert_eq!(prepared.decoded.samples.len(), 3);
    }

    #[test]
    fn playback_decode_tokens_cancel_previous_asset_loads() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let mut coordinator = PlaybackDecodeCoordinator::new();

        let first_token = coordinator.begin_decode(first, 0);
        let second_token = coordinator.begin_decode(second, 40);

        assert!(coordinator.is_cancelled(&first_token));
        assert!(!coordinator.is_cancelled(&second_token));
        assert_eq!(coordinator.active_asset_id(), Some(second));
    }

    #[test]
    fn playback_startup_latency_passes_under_one_hundred_ms() {
        assert_eq!(
            classify_playback_startup_latency(PlaybackStartupMeasurement {
                requested_at_ms: 1_000,
                started_at_ms: 1_080,
            }),
            PlaybackStartupLatency::Passed { elapsed_ms: 80 }
        );
        assert_eq!(
            classify_playback_startup_latency(PlaybackStartupMeasurement {
                requested_at_ms: 1_000,
                started_at_ms: 1_125,
            }),
            PlaybackStartupLatency::TooSlow { elapsed_ms: 125 }
        );
    }

    #[test]
    fn non_cached_preview_sources_do_not_decode_preview_audio() {
        let asset_id = Uuid::new_v4();

        assert_eq!(
            prepare_cached_preview_playback(&PlaybackSource::Original {
                asset_id,
                path: "/library/Media/00/hit.wav".to_string(),
            })
            .expect("prepare"),
            None
        );
        assert_eq!(
            prepare_cached_preview_playback(&PlaybackSource::Unavailable { asset_id })
                .expect("prepare"),
            None
        );
    }

    fn unique_wav_path() -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("darkwave-preview-{}.wav", Uuid::new_v4()));
        path
    }

    fn wav_16_bit_fixture() -> Vec<u8> {
        let samples = [-32768i16, 0, 32767];
        let data_size = samples.len() as u32 * 2;
        let mut wav = Vec::new();

        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_size).to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&48_000u32.to_le_bytes());
        wav.extend_from_slice(&96_000u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_size.to_le_bytes());
        for sample in samples {
            wav.extend_from_slice(&sample.to_le_bytes());
        }

        wav
    }
}
