//! Extracts license-validity dates from a purchased track's bundled PDF
//! (license terms, usage rights, receipt). Deliberately a *suggestion*
//! engine, not an authority: marketplace PDFs vary wildly in layout, date
//! format, and language, and some are scanned images with no extractable
//! text at all. Callers must always let the user confirm/override whatever
//! comes out of here before treating a date as the real expiry — see the
//! `attach_license_document` Tauri command, which never writes a
//! `DateCandidate` straight into storage.
use chrono::{NaiveDate, Utc};
use regex::{Regex, RegexBuilder};
use std::sync::LazyLock;

/// Today's date as `YYYY-MM-DD`, for comparing against `license_valid_until`
/// — exposed here (rather than requiring the desktop shell to depend on
/// `chrono` directly just for this) since this crate already carries the
/// dependency. Deliberately UTC, matching the rest of the app's timestamp
/// convention (`storage`'s `Utc::now().to_rfc3339()`); expiry checks only
/// need day granularity, so the UTC/local distinction doesn't matter here.
pub fn today_iso() -> String {
    Utc::now().date_naive().to_string()
}

#[derive(Debug)]
pub enum LicenseDocumentError {
    Extraction(String),
}

impl std::fmt::Display for LicenseDocumentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LicenseDocumentError::Extraction(message) => {
                write!(f, "failed to extract PDF text: {message}")
            }
        }
    }
}

impl std::error::Error for LicenseDocumentError {}

/// A date found near an expiry-suggesting keyword, never applied
/// automatically — the UI offers it as a one-click suggestion the user
/// still has to confirm.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DateCandidate {
    pub date: NaiveDate,
    /// The keyword phrase that put this date in play (e.g. "valid until").
    pub keyword: &'static str,
    /// A short slice of surrounding text, for the user to sanity-check the
    /// suggestion against (dates near "purchased on" and "valid until" can
    /// both appear on the same page).
    pub context: String,
    /// Higher ranks first — an exact "expires"/"valid until" match outranks
    /// a bare date sitting near a weaker keyword like "license period".
    rank: u8,
}

/// Reads raw text out of a PDF's bytes. Best-effort by nature — encrypted,
/// malformed, or scanned/image-only PDFs (no OCR here) legitimately return
/// an error or empty text; callers should treat that as "no candidates
/// found," not as a reason to fail whatever they were doing with the file.
pub fn extract_text(pdf_bytes: &[u8]) -> Result<String, LicenseDocumentError> {
    pdf_extract::extract_text_from_mem(pdf_bytes).map_err(|error| LicenseDocumentError::Extraction(error.to_string()))
}

const MONTH_NAMES: [(&str, u32); 12] = [
    ("january", 1),
    ("february", 2),
    ("march", 3),
    ("april", 4),
    ("may", 5),
    ("june", 6),
    ("july", 7),
    ("august", 8),
    ("september", 9),
    ("october", 10),
    ("november", 11),
    ("december", 12),
]; // matched by prefix against the abbreviated regex group (e.g. "Jan" vs "January")

// (keyword phrase, rank) — higher rank means a more explicit signal that
// the nearby date really is an expiry, not a purchase/issue date.
const KEYWORDS: [(&str, u8); 8] = [
    ("valid until", 3),
    ("valid through", 3),
    ("expiration date", 3),
    ("expiry date", 3),
    ("expires", 2),
    ("expiry", 2),
    ("license period", 1),
    ("term ends", 1),
];

static ISO_DATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d{4})-(\d{1,2})-(\d{1,2})").unwrap());
static SLASH_DATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d{1,2})/(\d{1,2})/(\d{4})").unwrap());
static MONTH_DAY_YEAR: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"(January|February|March|April|May|June|July|August|September|October|November|December|Jan|Feb|Mar|Apr|Jun|Jul|Aug|Sep|Sept|Oct|Nov|Dec)\.?\s+(\d{1,2}),?\s+(\d{4})",
    )
    .case_insensitive(true)
    .build()
    .unwrap()
});
static DAY_MONTH_YEAR: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"(\d{1,2})\s+(January|February|March|April|May|June|July|August|September|October|November|December|Jan|Feb|Mar|Apr|Jun|Jul|Aug|Sep|Sept|Oct|Nov|Dec)\.?,?\s+(\d{4})",
    )
    .case_insensitive(true)
    .build()
    .unwrap()
});

// Case-insensitive, built once, matched directly against the original text
// (never a separately-lowercased copy) — that keeps every byte offset used
// below a real offset into `text`, so a keyword window can never land on a
// non-UTF-8 boundary even when the document contains non-ASCII characters
// that change length under naive lowercasing.
static KEYWORD_PATTERNS: LazyLock<Vec<(Regex, &'static str, u8)>> = LazyLock::new(|| {
    KEYWORDS
        .iter()
        .map(|(keyword, rank)| {
            let pattern = RegexBuilder::new(&regex::escape(keyword))
                .case_insensitive(true)
                .build()
                .expect("keyword patterns are fixed literals");
            (pattern, *keyword, *rank)
        })
        .collect()
});

fn month_from_name(name: &str) -> Option<u32> {
    let lower = name.to_lowercase();
    MONTH_NAMES
        .iter()
        .find(|(full, _)| full.starts_with(&lower) || lower.starts_with(&full[..3]))
        .map(|(_, number)| *number)
}

/// Finds the first parseable date inside `window`, trying formats from
/// least to most ambiguous. `DD/MM/YYYY` is tried before `MM/DD/YYYY` for
/// the slash format since these are marketplace/EU-style license PDFs more
/// often than US-style ones — still just a heuristic, which is exactly why
/// every result here stays a suggestion rather than an authority.
fn first_date_in(window: &str) -> Option<NaiveDate> {
    if let Some(captures) = ISO_DATE.captures(window) {
        let year: i32 = captures[1].parse().ok()?;
        let month: u32 = captures[2].parse().ok()?;
        let day: u32 = captures[3].parse().ok()?;
        if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
            return Some(date);
        }
    }

    if let Some(captures) = MONTH_DAY_YEAR.captures(window) {
        let month = month_from_name(&captures[1])?;
        let day: u32 = captures[2].parse().ok()?;
        let year: i32 = captures[3].parse().ok()?;
        if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
            return Some(date);
        }
    }

    if let Some(captures) = DAY_MONTH_YEAR.captures(window) {
        let day: u32 = captures[1].parse().ok()?;
        let month = month_from_name(&captures[2])?;
        let year: i32 = captures[3].parse().ok()?;
        if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
            return Some(date);
        }
    }

    if let Some(captures) = SLASH_DATE.captures(window) {
        let (a, b, year): (u32, u32, i32) = (
            captures[1].parse().ok()?,
            captures[2].parse().ok()?,
            captures[3].parse().ok()?,
        );
        if let Some(date) = NaiveDate::from_ymd_opt(year, b, a) {
            return Some(date); // DD/MM/YYYY
        }
        if let Some(date) = NaiveDate::from_ymd_opt(year, a, b) {
            return Some(date); // fall back to MM/DD/YYYY
        }
    }

    None
}

/// Scans `text` for dates sitting near an expiry-suggesting keyword.
/// Returns candidates deduplicated by date (keeping the highest-ranked
/// occurrence), sorted by rank then by position in the document.
/// `str::floor_char_boundary` isn't stable yet — this is the same
/// walk-backwards trick, needed because keyword offsets are computed with
/// plain byte arithmetic (`+ 60`, `- 20`) that can otherwise land inside a
/// multi-byte character in non-ASCII license text and panic on slicing.
fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    let len = text.len();
    if index >= len {
        return len;
    }
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

pub fn find_date_candidates(text: &str) -> Vec<DateCandidate> {
    let mut candidates: Vec<DateCandidate> = Vec::new();

    for (pattern, keyword, rank) in KEYWORD_PATTERNS.iter() {
        for keyword_match in pattern.find_iter(text) {
            let keyword_end = keyword_match.end();
            // A window after the keyword is enough for every layout seen
            // in practice ("Valid until: 5 January 2027", "Expires
            // 2027-01-05") without wandering into an unrelated sentence.
            let window_end = floor_char_boundary(text, (keyword_end + 60).min(text.len()));
            let window = &text[keyword_end..window_end];

            if let Some(date) = first_date_in(window) {
                let context_start = floor_char_boundary(text, keyword_match.start().saturating_sub(20));
                let context = text[context_start..window_end].trim().to_string();

                candidates.push(DateCandidate {
                    date,
                    keyword,
                    context,
                    rank: *rank,
                });
            }
        }
    }

    candidates.sort_by(|a, b| b.rank.cmp(&a.rank).then(a.date.cmp(&b.date)));

    let mut seen = std::collections::HashSet::new();
    candidates.retain(|candidate| seen.insert(candidate.date));

    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    fn escape_pdf_string(value: &str) -> String {
        value.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)")
    }

    /// Builds a minimal, structurally valid single-page PDF containing
    /// `text`, with byte-exact xref offsets computed from the buffer as
    /// it's written (not hand-counted), so this stays correct if the
    /// template ever changes.
    fn build_minimal_pdf(text: &str) -> Vec<u8> {
        let stream_content = format!("BT /F1 12 Tf 72 712 Td ({}) Tj ET", escape_pdf_string(text));
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 4 0 R >> >> /MediaBox [0 0 612 792] /Contents 5 0 R >>"
                .to_string(),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
            format!("<< /Length {} >>\nstream\n{}\nendstream", stream_content.len(), stream_content),
        ];

        let mut buffer: Vec<u8> = Vec::new();
        buffer.extend_from_slice(b"%PDF-1.4\n");

        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(buffer.len());
            buffer.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, object).as_bytes());
        }

        let xref_offset = buffer.len();
        buffer.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
        buffer.extend_from_slice(b"0000000000 65535 f \n");
        for offset in &offsets {
            buffer.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        buffer.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF",
                objects.len() + 1,
                xref_offset
            )
            .as_bytes(),
        );

        buffer
    }

    #[test]
    fn extracts_text_from_a_minimal_pdf() {
        let pdf = build_minimal_pdf("This license is valid until 2027-01-05.");
        let text = extract_text(&pdf).expect("a well-formed PDF should extract cleanly");
        assert!(text.contains("valid until"), "extracted text was: {text:?}");
    }

    #[test]
    fn garbage_bytes_return_an_error_not_a_panic() {
        let result = extract_text(b"not a pdf at all");
        assert!(result.is_err());
    }

    #[test]
    fn finds_iso_date_after_valid_until() {
        let candidates = find_date_candidates("Terms: this license is Valid Until 2027-01-05 for the buyer.");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].date, NaiveDate::from_ymd_opt(2027, 1, 5).unwrap());
        assert_eq!(candidates[0].keyword, "valid until");
    }

    #[test]
    fn finds_long_month_name_date_after_expires() {
        let candidates = find_date_candidates("Your license expires on January 5, 2027 unless renewed.");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].date, NaiveDate::from_ymd_opt(2027, 1, 5).unwrap());
    }

    #[test]
    fn finds_day_month_year_date_after_expiry_date() {
        let candidates = find_date_candidates("Expiry date: 5 Jan 2027.");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].date, NaiveDate::from_ymd_opt(2027, 1, 5).unwrap());
    }

    #[test]
    fn slash_date_prefers_day_month_year() {
        // 13 can't be a month, so this is unambiguous either way — a good
        // sanity check that the day/month assignment isn't swapped.
        let candidates = find_date_candidates("Valid until 13/01/2027.");
        assert_eq!(candidates[0].date, NaiveDate::from_ymd_opt(2027, 1, 13).unwrap());
    }

    #[test]
    fn ranks_explicit_expiry_keywords_above_weaker_ones() {
        let text = "License period runs to 2026-06-01. This license is valid until 2027-01-05.";
        let candidates = find_date_candidates(text);
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].date, NaiveDate::from_ymd_opt(2027, 1, 5).unwrap());
        assert_eq!(candidates[1].date, NaiveDate::from_ymd_opt(2026, 6, 1).unwrap());
    }

    #[test]
    fn deduplicates_the_same_date_found_under_multiple_keywords() {
        let text = "Expires 2027-01-05. Valid until 2027-01-05.";
        let candidates = find_date_candidates(text);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].keyword, "valid until");
    }

    #[test]
    fn text_with_no_keywords_returns_no_candidates() {
        let candidates = find_date_candidates("Purchased on 2026-01-01. Thanks for your order!");
        assert!(candidates.is_empty());
    }
}
