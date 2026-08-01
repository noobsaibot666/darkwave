//! Isolated GPL-3.0 subprocess: analyzes audio files with bliss-rs and
//! prints each similarity feature vector as JSON on stdout. Spawned by the
//! (Proprietary-licensed) main app as a Tauri sidecar rather than linked in
//! -process, so GPL-3.0 code never enters that binary's link graph. See
//! ../README.md and docs/adr/0025-real-audio-analysis.md.
//!
//! Two modes:
//! - `similarity-worker <path-to-audio-file>` — analyze one file and exit.
//!   Stdout (one line): {"analysis":[f32; N]} on success, {"error":"..."} on
//!   failure.
//! - `similarity-worker --stdin-loop` — stay resident, reading one file path
//!   per line from stdin and printing one response line (same schema as
//!   above) per path, until stdin closes. Lets the caller reuse a single
//!   process across a whole analysis batch instead of paying process-start
//!   cost per file — see docs/adr and apps/desktop/src-tauri's
//!   `run_similarity_worker`, which is the only real caller of this mode.

use bliss_audio::decoder::symphonia::SymphoniaDecoder;
use bliss_audio::decoder::Decoder;
use serde_json::json;
use std::io::{BufRead, Write};

fn main() {
    match std::env::args().nth(1) {
        Some(flag) if flag == "--stdin-loop" => run_stdin_loop(),
        Some(path) => run_single(&path),
        None => {
            print_error("usage: similarity-worker <path-to-audio-file> | similarity-worker --stdin-loop");
            std::process::exit(2);
        }
    }
}

fn run_single(path: &str) {
    match analyze(path) {
        Ok(analysis) => println!("{}", json!({ "analysis": analysis })),
        Err(error) => {
            print_error(&error);
            std::process::exit(1);
        }
    }
}

fn run_stdin_loop() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();

    for line in stdin.lock().lines() {
        let Ok(path) = line else { break };
        let path = path.trim();
        if path.is_empty() {
            continue;
        }

        let response = match analyze(path) {
            Ok(analysis) => json!({ "analysis": analysis }),
            Err(error) => json!({ "error": error }),
        };

        // The parent reads responses split on newlines from a pipe that
        // isn't a TTY, so stdout is block-buffered by default — without an
        // explicit flush a response can sit in this process's buffer
        // instead of reaching the parent, and it hangs waiting for a line
        // that was already "written."
        let mut out = stdout.lock();
        let _ = writeln!(out, "{response}");
        let _ = out.flush();
    }
}

fn analyze(path: &str) -> Result<Vec<f32>, String> {
    SymphoniaDecoder::song_from_path(path)
        .map(|song| song.analysis.as_vec())
        .map_err(|error| error.to_string())
}

fn print_error(message: &str) {
    println!("{}", json!({ "error": message }));
}
