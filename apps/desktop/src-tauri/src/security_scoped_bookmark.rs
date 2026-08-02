// Mac App Store build only. Darkwave requires the user to explicitly pick
// each library's source folder via a native panel (Tauri's dialog plugin)
// rather than scanning arbitrary paths, which is exactly the case macOS
// security-scoped bookmarks exist for: the sandbox grants access to a
// just-picked folder for the running process, and a bookmark is the
// supported way to regain that same access on a later launch — including
// after an SMB/NAS share remounts at a different underlying path — without
// a fresh folder-picker prompt every time.
//
// Not compiled for the direct-dist build at all: it's unsandboxed and has
// unrestricted filesystem access already, so there's nothing here for it
// to do. (objc2-foundation is still present in that build's dependency
// graph — see the comment on it in Cargo.toml — just unused.)
#![cfg(all(target_os = "macos", not(feature = "direct-dist")))]

use std::path::{Path, PathBuf};

use base64::{engine::general_purpose, Engine as _};
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_foundation::{NSData, NSURLBookmarkCreationOptions, NSURLBookmarkResolutionOptions, NSURL};

/// Creates a security-scoped bookmark for `path`, base64-encoded for
/// storage in `storage::LibraryRecord::media_root_bookmark`. Must be called
/// with a path the sandbox has *just* granted access to (i.e. the return
/// value of a folder-picker dialog in the same session) — `bookmarkData...`
/// with `WithSecurityScope` only succeeds against a URL the process
/// currently has scoped access to.
pub fn create_bookmark(path: &Path) -> Result<String, String> {
    let url = NSURL::from_file_path(path)
        .ok_or_else(|| format!("not a valid file URL: {}", path.display()))?;

    let data = url
        .bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
            NSURLBookmarkCreationOptions::WithSecurityScope,
            None,
            None,
        )
        .map_err(|error| error.to_string())?;

    Ok(general_purpose::STANDARD.encode(data.to_vec()))
}

/// Resolves a bookmark created by [`create_bookmark`] and starts
/// security-scoped access. Hold the returned [`BookmarkAccess`] for as long
/// as the library's files need to be reachable — dropping it calls
/// `stopAccessingSecurityScopedResource`, after which further reads outside
/// the sandbox's own extent will fail again.
pub struct BookmarkAccess {
    url: Retained<NSURL>,
}

impl BookmarkAccess {
    pub fn path(&self) -> Option<PathBuf> {
        self.url.to_file_path()
    }
}

impl Drop for BookmarkAccess {
    fn drop(&mut self) {
        // SAFETY: `url` was returned by a successful
        // startAccessingSecurityScopedResource call in `resolve_bookmark`,
        // which is the documented precondition for calling stop.
        unsafe { self.url.stopAccessingSecurityScopedResource() };
    }
}

pub fn resolve_bookmark(bookmark_base64: &str) -> Result<BookmarkAccess, String> {
    let bytes = general_purpose::STANDARD
        .decode(bookmark_base64)
        .map_err(|error| error.to_string())?;
    let data = NSData::with_bytes(&bytes);

    let mut is_stale = Bool::NO;
    // SAFETY: `is_stale` is a valid, live `Bool` for the duration of the
    // call, matching the binding's documented safety requirement.
    let url = unsafe {
        NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
            &data,
            NSURLBookmarkResolutionOptions::WithSecurityScope,
            None,
            &mut is_stale as *mut Bool,
        )
    }
    .map_err(|error| error.to_string())?;

    // A stale bookmark (e.g. the NAS share remounted under a different
    // volume UUID) can still resolve successfully — the caller is
    // responsible for minting a fresh bookmark via `create_bookmark` on
    // this same path afterward so future launches don't keep resolving the
    // stale one. Not treated as an error here: the resolved URL is still
    // usable for this session.
    if is_stale.as_bool() {
        // Nothing to do beyond noting it — see doc comment above. Logged at
        // the call site (src-tauri/src/lib.rs), which has access to the
        // library id needed to actually refresh the stored bookmark.
    }

    // SAFETY: starting access is documented as safe to call on any
    // resolved security-scoped URL.
    let started = unsafe { url.startAccessingSecurityScopedResource() };
    if !started {
        return Err("could not start accessing the security-scoped resource".into());
    }

    Ok(BookmarkAccess { url })
}
