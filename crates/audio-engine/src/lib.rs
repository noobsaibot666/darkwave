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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_commands_outrank_background_work() {
        assert_eq!(command_priority(PlaybackCommand::Play), 0);
    }
}
