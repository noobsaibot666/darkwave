use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PeakLevel {
    pub min: f32,
    pub max: f32,
}

/// Bucket count for the compact transport-bar strip the desktop shell
/// renders — a flat magnitude array, one value per bar. Kept here so the
/// backend generator and any client-side fallback agree on length.
pub const TRANSPORT_STRIP_BUCKETS: usize = 200;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WaveformCache {
    pub sample_rate: u32,
    pub row: Vec<PeakLevel>,
    pub inspector: Vec<PeakLevel>,
    pub transport: Vec<PeakLevel>,
}

impl WaveformCache {
    pub fn from_samples(samples: &[f32], sample_rate: u32) -> Self {
        // Each resolution is generated directly from `samples`, not via a
        // one-bucket-per-sample "full" array downsampled three times after
        // the fact — that intermediate used to be samples.len() long (tens
        // of millions of PeakLevel entries for an ordinary multi-minute
        // track), all of it discarded immediately after. Building only the
        // three bounded resolutions actually needed cuts both the memory
        // churn and the redundant passes over the sample buffer.
        Self {
            sample_rate,
            row: generate_peaks(samples, 128),
            inspector: generate_peaks(samples, 512),
            transport: generate_peaks(samples, 2048),
        }
    }

    /// Flat per-bucket magnitudes (0.0..=1.0) for the compact transport
    /// strip — `max(|min|, |max|)` of each bucket, derived from the
    /// already-downsampled `transport` layer so this stays cheap. Always
    /// exactly `TRANSPORT_STRIP_BUCKETS` long (zero-padded for a clip too
    /// short or silent to fill it), so a persisted strip is always the
    /// authoritative one and the UI never has to recompute for a quiet file.
    pub fn transport_strip(&self) -> Vec<f32> {
        let mut strip = peak_magnitudes(&self.transport, TRANSPORT_STRIP_BUCKETS);
        strip.resize(TRANSPORT_STRIP_BUCKETS, 0.0);
        strip
    }
}

/// Collapses a peak array to `target_len` flat magnitude values in
/// `0.0..=1.0`. Used for the transport strip and for validating a
/// client-supplied fallback array before it's persisted.
pub fn peak_magnitudes(peaks: &[PeakLevel], target_len: usize) -> Vec<f32> {
    downsample_peaks(peaks, target_len)
        .into_iter()
        .map(|peak| peak.min.abs().max(peak.max.abs()).clamp(0.0, 1.0))
        .collect()
}

pub fn generate_peaks(samples: &[f32], target_len: usize) -> Vec<PeakLevel> {
    if target_len == 0 || samples.is_empty() {
        return Vec::new();
    }

    let chunk_size = samples.len().div_ceil(target_len);
    samples
        .chunks(chunk_size)
        .map(|chunk| {
            let mut min = 1.0_f32;
            let mut max = -1.0_f32;

            for sample in chunk {
                let sample = sample.clamp(-1.0, 1.0);
                min = min.min(sample);
                max = max.max(sample);
            }

            PeakLevel { min, max }
        })
        .collect()
}

pub fn downsample_peaks(peaks: &[PeakLevel], target_len: usize) -> Vec<PeakLevel> {
    if target_len == 0 || peaks.is_empty() {
        return Vec::new();
    }

    let chunk_size = peaks.len().div_ceil(target_len);
    peaks
        .chunks(chunk_size)
        .map(|chunk| PeakLevel {
            min: chunk.iter().map(|peak| peak.min).fold(1.0, f32::min),
            max: chunk.iter().map(|peak| peak.max).fold(-1.0, f32::max),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downsample_preserves_extremes() {
        let peaks = vec![
            PeakLevel {
                min: -0.2,
                max: 0.4,
            },
            PeakLevel {
                min: -0.7,
                max: 0.2,
            },
        ];

        assert_eq!(
            downsample_peaks(&peaks, 1),
            vec![PeakLevel {
                min: -0.7,
                max: 0.4
            }]
        );
    }

    #[test]
    fn generates_bounded_peak_data_from_samples() {
        let peaks = generate_peaks(&[-2.0, -0.5, 0.25, 2.0], 2);

        assert_eq!(
            peaks,
            vec![
                PeakLevel {
                    min: -1.0,
                    max: -0.5
                },
                PeakLevel {
                    min: 0.25,
                    max: 1.0
                },
            ]
        );
    }

    #[test]
    fn creates_multi_resolution_waveform_cache_payload() {
        let cache = WaveformCache::from_samples(&[-1.0, -0.25, 0.5, 1.0], 44_100);

        assert_eq!(cache.sample_rate, 44_100);
        assert_eq!(cache.row.len(), 4);
        assert_eq!(cache.inspector.len(), 4);
        assert_eq!(cache.transport.len(), 4);
    }

    #[test]
    fn peak_magnitudes_collapses_to_positive_envelope() {
        let peaks = vec![
            PeakLevel { min: -0.8, max: 0.2 },
            PeakLevel { min: -0.1, max: 0.5 },
        ];

        assert_eq!(peak_magnitudes(&peaks, 2), vec![0.8, 0.5]);
    }

    #[test]
    fn transport_strip_is_bounded_and_never_negative() {
        let samples: Vec<f32> = (0..10_000)
            .map(|i| if i % 2 == 0 { -0.9 } else { 0.7 })
            .collect();
        let strip = WaveformCache::from_samples(&samples, 48_000).transport_strip();

        assert_eq!(strip.len(), TRANSPORT_STRIP_BUCKETS);
        assert!(strip.iter().all(|value| (0.0..=1.0).contains(value)));
    }

    #[test]
    fn transport_strip_is_full_length_even_for_a_tiny_or_silent_clip() {
        // Fewer source samples than buckets, and an all-zero clip.
        assert_eq!(
            WaveformCache::from_samples(&[0.4, -0.6, 0.1], 44_100)
                .transport_strip()
                .len(),
            TRANSPORT_STRIP_BUCKETS
        );
        let silent = WaveformCache::from_samples(&[0.0; 5_000], 44_100).transport_strip();
        assert_eq!(silent.len(), TRANSPORT_STRIP_BUCKETS);
        assert!(silent.iter().all(|value| *value == 0.0));
    }

    #[test]
    fn waveform_cache_round_trips_through_json() {
        let cache = WaveformCache::from_samples(&[-1.0, -0.25, 0.5, 1.0], 44_100);
        let json = serde_json::to_string(&cache).expect("serialize");
        let restored: WaveformCache = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(cache, restored);
    }
}
