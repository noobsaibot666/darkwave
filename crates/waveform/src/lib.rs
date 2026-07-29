#[derive(Clone, Debug, PartialEq)]
pub struct PeakLevel {
    pub min: f32,
    pub max: f32,
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
}
