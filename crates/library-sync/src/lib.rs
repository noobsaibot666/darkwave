#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WriterLeaseState {
    Writable,
    ReadOnlyBecauseAnotherWriterExists,
}

pub fn lease_state(active_writer_device: Option<&str>, current_device: &str) -> WriterLeaseState {
    match active_writer_device {
        Some(device) if device != current_device => {
            WriterLeaseState::ReadOnlyBecauseAnotherWriterExists
        }
        _ => WriterLeaseState::Writable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflicting_writer_forces_read_only_mode() {
        assert_eq!(
            lease_state(Some("edit-suite"), "laptop"),
            WriterLeaseState::ReadOnlyBecauseAnotherWriterExists
        );
    }
}
