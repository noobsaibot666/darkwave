#[derive(Clone, Debug, PartialEq)]
pub struct PeakLevel {
    pub min: f32,
    pub max: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WaveformCache {
    pub sample_rate: u32,
    pub row: Vec<PeakLevel>,
    pub inspector: Vec<PeakLevel>,
    pub transport: Vec<PeakLevel>,
}

impl WaveformCache {
    pub fn from_samples(samples: &[f32], sample_rate: u32) -> Self {
        let full = generate_peaks(samples, samples.len().max(1));

        Self {
            sample_rate,
            row: downsample_peaks(&full, 128),
            inspector: downsample_peaks(&full, 512),
            transport: downsample_peaks(&full, 2048),
        }
    }
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
}
