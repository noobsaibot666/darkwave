//! Isolated GPL-3.0 subprocess: analyzes one audio file with bliss-rs and
//! prints its similarity feature vector as JSON on stdout. Spawned by the
//! (Proprietary-licensed) main app as a Tauri sidecar rather than linked in
//! -process, so GPL-3.0 code never enters that binary's link graph. See
//! ../README.md and docs/adr/0025-real-audio-analysis.md.
//!
//! Usage: similarity-worker <path-to-audio-file>
//! Stdout (one line): {"analysis":[f32; N]} on success, {"error":"..."} on failure.

use bliss_audio::decoder::symphonia::SymphoniaDecoder;
use bliss_audio::decoder::Decoder;
use serde_json::json;

fn main() {
    let path = match std::env::args().nth(1) {
        Some(path) => path,
        None => {
            print_error("usage: similarity-worker <path-to-audio-file>");
            std::process::exit(2);
        }
    };

    match SymphoniaDecoder::song_from_path(&path) {
        Ok(song) => {
            let analysis = song.analysis.as_vec();
            println!("{}", json!({ "analysis": analysis }));
        }
        Err(error) => {
            print_error(&error.to_string());
            std::process::exit(1);
        }
    }
}

fn print_error(message: &str) {
    println!("{}", json!({ "error": message }));
}
