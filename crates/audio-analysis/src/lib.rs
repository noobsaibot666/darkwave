#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioMeasurements {
    pub peak_db: f32,
    pub transient_density: f32,
    pub low_frequency_energy: f32,
}

pub fn intensity_score(measurements: AudioMeasurements) -> f32 {
    (measurements.transient_density * 0.5
        + measurements.low_frequency_energy * 0.3
        + measurements.peak_db.abs().recip() * 0.2)
        .clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intensity_score_is_bounded() {
        let score = intensity_score(AudioMeasurements {
            peak_db: -0.1,
            transient_density: 9.0,
            low_frequency_energy: 9.0,
        });

        assert_eq!(score, 1.0);
    }
}
