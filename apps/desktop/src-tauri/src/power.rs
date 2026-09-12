//! Prevents the OS from idle-sleeping the machine while there's unpaused
//! background analysis work pending — see the "overnight batch" ask this
//! exists for. Deliberately narrow: only idle *system* sleep is prevented
//! (display sleep, lid-close, and "survive the app window closing" are all
//! untouched), and the assertion is held only while there's real work to
//! do, so it engages/releases in step with the job queue rather than for
//! the app's whole lifetime.

#[cfg(target_os = "macos")]
mod imp {
    use std::process::{Child, Command};
    use std::sync::Mutex;

    pub struct Assertion(Mutex<Option<Child>>);

    impl Assertion {
        pub fn new() -> Self {
            Self(Mutex::new(None))
        }

        /// `-i` prevents idle system sleep specifically (not display sleep,
        /// and — unlike `-s` — works on battery too), which is exactly the
        /// scope wanted: the screen can still turn off, but the machine
        /// (and this process) must not suspend mid-batch.
        pub fn engage(&self) {
            let mut guard = self.0.lock().expect("power assertion mutex poisoned");
            if guard.is_some() {
                return;
            }
            match Command::new("/usr/bin/caffeinate").arg("-i").spawn() {
                Ok(child) => *guard = Some(child),
                Err(error) => eprintln!("power: failed to spawn caffeinate: {error}"),
            }
        }

        pub fn release(&self) {
            let mut guard = self.0.lock().expect("power assertion mutex poisoned");
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }

        #[cfg(test)]
        pub fn child_pid(&self) -> Option<u32> {
            self.0.lock().expect("power assertion mutex poisoned").as_ref().map(Child::id)
        }
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use std::sync::atomic::{AtomicBool, Ordering};

    #[link(name = "kernel32")]
    extern "system" {
        fn SetThreadExecutionState(esFlags: u32) -> u32;
    }

    const ES_CONTINUOUS: u32 = 0x8000_0000;
    const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;

    pub struct Assertion(AtomicBool);

    impl Assertion {
        pub fn new() -> Self {
            Self(AtomicBool::new(false))
        }

        pub fn engage(&self) {
            if self.0.swap(true, Ordering::SeqCst) {
                return;
            }
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
            }
        }

        pub fn release(&self) {
            if !self.0.swap(false, Ordering::SeqCst) {
                return;
            }
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS);
            }
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod imp {
    pub struct Assertion;

    impl Assertion {
        pub fn new() -> Self {
            Self
        }

        pub fn engage(&self) {}

        pub fn release(&self) {}
    }
}

/// Tauri-managed state wrapping the platform assertion. `engage`/`release`
/// are idempotent and cheap to call every drive-loop tick.
pub struct PowerAssertionState(imp::Assertion);

impl PowerAssertionState {
    pub fn new() -> Self {
        Self(imp::Assertion::new())
    }

    pub fn engage(&self) {
        self.0.engage();
    }

    pub fn release(&self) {
        self.0.release();
    }

    #[cfg(all(test, target_os = "macos"))]
    fn child_pid(&self) -> Option<u32> {
        self.0.child_pid()
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    fn pid_is_alive(pid: u32) -> bool {
        std::process::Command::new("ps")
            .args(["-p", &pid.to_string()])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn engage_spawns_caffeinate_and_release_kills_it() {
        let state = PowerAssertionState::new();

        state.engage();
        let pid = state.child_pid().expect("engage() should have spawned a child");
        assert!(pid_is_alive(pid), "caffeinate should be running after engage()");

        state.release();
        // kill() + wait() inside release() means this should already be reaped.
        assert!(!pid_is_alive(pid), "caffeinate should be gone after release()");
    }

    #[test]
    fn engage_is_idempotent_no_duplicate_process() {
        let state = PowerAssertionState::new();
        state.engage();
        let first_pid = state.child_pid();
        state.engage();
        let second_pid = state.child_pid();
        assert_eq!(first_pid, second_pid, "a second engage() must not spawn another process");
        state.release();
    }
}
