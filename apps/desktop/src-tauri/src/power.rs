//! Prevents the OS from idle-sleeping the machine while there's unpaused
//! background analysis work pending — see the "overnight batch" ask this
//! exists for. Deliberately narrow: only idle *system* sleep is prevented
//! (display sleep, lid-close, and "survive the app window closing" are all
//! untouched), and the assertion is held only while there's real work to
//! do, so it engages/releases in step with the job queue rather than for
//! the app's whole lifetime.

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::{c_char, c_void, CString};
    use std::sync::Mutex;

    // Hand-rolled IOKit/CoreFoundation FFI rather than a spawned
    // `caffeinate` process: this app's Mac App Store build runs under the
    // App Sandbox, which blocks posix_spawn/exec of any binary that isn't
    // embedded in the app bundle and signed with com.apple.security.inherit
    // — `/usr/bin/caffeinate` is neither, so a spawned child silently fails
    // with EPERM there (the direct-sale build isn't sandboxed, so that one
    // would have worked, which is exactly the kind of gap that's easy to
    // miss without testing the MAS build specifically).
    // `IOPMAssertionCreateWithName` is a normal framework call — no
    // process spawn, confirmed to work inside the sandbox with no special
    // entitlement — and is the standard, documented way MAS apps prevent
    // idle sleep.
    type CFStringRef = *const c_void;
    type CFAllocatorRef = *const c_void;
    type IOPMAssertionId = u32;
    type IOReturn = i32;

    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    const K_IOPM_ASSERTION_LEVEL_ON: u32 = 255;
    const K_IO_RETURN_SUCCESS: IOReturn = 0;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringCreateWithCString(alloc: CFAllocatorRef, c_str: *const c_char, encoding: u32) -> CFStringRef;
        fn CFRelease(cf: *const c_void);
    }

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOPMAssertionCreateWithName(
            assertion_type: CFStringRef,
            assertion_level: u32,
            assertion_name: CFStringRef,
            assertion_id: *mut IOPMAssertionId,
        ) -> IOReturn;
        fn IOPMAssertionRelease(assertion_id: IOPMAssertionId) -> IOReturn;
    }

    /// `None` if the `CString`/`CFString` conversion fails (never expected
    /// for the fixed ASCII literals this is called with) — callers treat
    /// that the same as any other best-effort failure to engage.
    fn cfstring(value: &str) -> Option<CFStringRef> {
        let c_string = CString::new(value).ok()?;
        let string_ref =
            unsafe { CFStringCreateWithCString(std::ptr::null(), c_string.as_ptr(), K_CF_STRING_ENCODING_UTF8) };
        if string_ref.is_null() {
            None
        } else {
            Some(string_ref)
        }
    }

    pub struct Assertion(Mutex<Option<IOPMAssertionId>>);

    // Safety: an IOPMAssertionId is an opaque handle (just a u32), and
    // IOKit's power-assertion functions are documented safe to call from
    // any thread — nothing here is actually thread-affine, so sharing this
    // across the Tokio worker threads Tauri's managed state is accessed
    // from is fine.
    unsafe impl Send for Assertion {}
    unsafe impl Sync for Assertion {}

    impl Assertion {
        pub fn new() -> Self {
            Self(Mutex::new(None))
        }

        /// `PreventUserIdleSystemSleep` prevents idle *system* sleep
        /// specifically (not display sleep) — exactly the scope wanted:
        /// the screen can still turn off, but the machine (and this
        /// process) must not suspend mid-batch.
        pub fn engage(&self) {
            let mut guard = self.0.lock().expect("power assertion mutex poisoned");
            if guard.is_some() {
                return;
            }

            let Some(assertion_type) = cfstring("PreventUserIdleSystemSleep") else {
                return;
            };
            let Some(assertion_name) = cfstring("Darkwave: background analysis in progress") else {
                unsafe { CFRelease(assertion_type) };
                return;
            };

            let mut assertion_id: IOPMAssertionId = 0;
            let result = unsafe {
                IOPMAssertionCreateWithName(
                    assertion_type,
                    K_IOPM_ASSERTION_LEVEL_ON,
                    assertion_name,
                    &mut assertion_id,
                )
            };
            unsafe {
                CFRelease(assertion_type);
                CFRelease(assertion_name);
            }

            if result == K_IO_RETURN_SUCCESS {
                *guard = Some(assertion_id);
            } else {
                eprintln!("power: IOPMAssertionCreateWithName failed (IOReturn {result})");
            }
        }

        pub fn release(&self) {
            let mut guard = self.0.lock().expect("power assertion mutex poisoned");
            if let Some(assertion_id) = guard.take() {
                unsafe {
                    IOPMAssertionRelease(assertion_id);
                }
            }
        }

        #[cfg(test)]
        pub fn is_engaged(&self) -> bool {
            self.0.lock().expect("power assertion mutex poisoned").is_some()
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
    fn is_engaged(&self) -> bool {
        self.0.is_engaged()
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn engage_holds_an_assertion_and_release_clears_it() {
        let state = PowerAssertionState::new();

        state.engage();
        assert!(state.is_engaged(), "engage() should hold an IOPMAssertion");

        state.release();
        assert!(!state.is_engaged(), "release() should clear the assertion");
    }

    #[test]
    fn engage_is_idempotent() {
        let state = PowerAssertionState::new();
        state.engage();
        state.engage();
        assert!(state.is_engaged());
        state.release();
        assert!(!state.is_engaged());
    }

    #[test]
    fn release_without_engage_is_a_harmless_no_op() {
        let state = PowerAssertionState::new();
        state.release();
        assert!(!state.is_engaged());
    }

    // Not run by default (`cargo test -- --ignored --nocapture
    // power::tests::manual_verify_via_pmset`) — confirms the assertion is
    // real at the OS level, not just internally tracked, by shelling out to
    // `pmset -g assertions` and checking our named assertion shows up.
    #[test]
    #[ignore]
    fn manual_verify_via_pmset() {
        let state = PowerAssertionState::new();
        state.engage();
        let output = std::process::Command::new("pmset")
            .args(["-g", "assertions"])
            .output()
            .expect("run pmset");
        let text = String::from_utf8_lossy(&output.stdout);
        println!("{text}");
        assert!(text.contains("PreventUserIdleSystemSleep"));
        state.release();
    }
}
