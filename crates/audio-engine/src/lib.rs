use uuid::Uuid;

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
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PlaybackSession {
    active_asset_id: Option<Uuid>,
    duration_ms: u64,
    position_ms: u64,
    playing: bool,
    loop_region: Option<LoopRegion>,
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
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
