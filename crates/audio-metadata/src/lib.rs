use std::path::Path;
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileMetadata {
    pub extension: String,
    pub file_size: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EmbeddedFileMetadata {
    pub title: Option<String>,
    pub genre: Option<String>,
    pub comment: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecodedAudioBuffer {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecSupportStatus {
    NativePcm,
    RequiresPackagedDecoder,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodecSupport {
    pub extension: String,
    pub status: CodecSupportStatus,
    pub conversion_available: bool,
}

pub trait PackagedAudioDecoder {
    fn decode_packaged_audio(
        &self,
        path: &Path,
        extension: &str,
    ) -> Result<DecodedAudioBuffer, MetadataError>;
}

#[derive(Debug, Error)]
pub enum MetadataError {
    #[error("file has no extension")]
    MissingExtension,
    #[error("unsupported decoder format: {0}")]
    UnsupportedDecoderFormat(String),
    #[error("packaged decoder unavailable for format: {0}")]
    PackagedDecoderUnavailable(String),
    #[error("invalid wav data")]
    InvalidWav,
    #[error("metadata read failed: {0}")]
    Io(#[from] std::io::Error),
}

impl PartialEq for MetadataError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (MetadataError::MissingExtension, MetadataError::MissingExtension)
            | (MetadataError::InvalidWav, MetadataError::InvalidWav) => true,
            (
                MetadataError::UnsupportedDecoderFormat(left),
                MetadataError::UnsupportedDecoderFormat(right),
            )
            | (
                MetadataError::PackagedDecoderUnavailable(left),
                MetadataError::PackagedDecoderUnavailable(right),
            ) => left == right,
            _ => false,
        }
    }
}

impl Eq for MetadataError {}

pub fn supported_mvp_format(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "wav" | "aiff" | "aif" | "mp3" | "flac" | "aac" | "m4a" | "ogg"
    )
}

pub fn codec_support_for_extension(extension: &str) -> CodecSupport {
    let extension = extension.to_ascii_lowercase();
    let status = match extension.as_str() {
        "wav" => CodecSupportStatus::NativePcm,
        "aiff" | "aif" | "mp3" | "flac" | "aac" | "m4a" | "ogg" => {
            CodecSupportStatus::RequiresPackagedDecoder
        }
        _ => CodecSupportStatus::Unsupported,
    };

    CodecSupport {
        extension,
        status,
        conversion_available: status != CodecSupportStatus::NativePcm,
    }
}

pub fn extract_immediate_metadata(path: impl AsRef<Path>) -> Result<FileMetadata, MetadataError> {
    let path = path.as_ref();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .ok_or(MetadataError::MissingExtension)?
        .to_ascii_lowercase();
    let metadata = std::fs::metadata(path)?;

    Ok(FileMetadata {
        extension,
        file_size: metadata.len(),
    })
}

pub fn extract_embedded_metadata(
    path: impl AsRef<Path>,
) -> Result<EmbeddedFileMetadata, MetadataError> {
    let path = path.as_ref();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .ok_or(MetadataError::MissingExtension)?
        .to_ascii_lowercase();

    if extension != "wav" {
        return Ok(EmbeddedFileMetadata::default());
    }

    let bytes = std::fs::read(path)?;
    parse_wav_info_metadata(&bytes)
}

pub fn decode_wav_pcm(path: impl AsRef<Path>) -> Result<DecodedAudioBuffer, MetadataError> {
    let path = path.as_ref();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .ok_or(MetadataError::MissingExtension)?
        .to_ascii_lowercase();
    if extension != "wav" {
        return Err(MetadataError::UnsupportedDecoderFormat(extension));
    }

    let bytes = std::fs::read(path)?;
    parse_wav_pcm(&bytes)
}

pub fn decode_supported_audio(
    path: impl AsRef<Path>,
    packaged_decoder: Option<&dyn PackagedAudioDecoder>,
) -> Result<DecodedAudioBuffer, MetadataError> {
    let path = path.as_ref();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .ok_or(MetadataError::MissingExtension)?
        .to_ascii_lowercase();

    match codec_support_for_extension(&extension).status {
        CodecSupportStatus::NativePcm => decode_wav_pcm(path),
        CodecSupportStatus::RequiresPackagedDecoder => packaged_decoder
            .ok_or_else(|| MetadataError::PackagedDecoderUnavailable(extension.clone()))?
            .decode_packaged_audio(path, &extension),
        CodecSupportStatus::Unsupported => Err(MetadataError::UnsupportedDecoderFormat(extension)),
    }
}

fn parse_wav_info_metadata(bytes: &[u8]) -> Result<EmbeddedFileMetadata, MetadataError> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(MetadataError::InvalidWav);
    }

    let mut metadata = EmbeddedFileMetadata::default();
    let mut cursor = 12usize;

    while cursor + 8 <= bytes.len() {
        let chunk_id = &bytes[cursor..cursor + 4];
        let chunk_size = read_u32_le(bytes, cursor + 4)? as usize;
        let chunk_start = cursor + 8;
        let chunk_end = chunk_start
            .checked_add(chunk_size)
            .ok_or(MetadataError::InvalidWav)?;
        if chunk_end > bytes.len() {
            return Err(MetadataError::InvalidWav);
        }

        if chunk_id == b"LIST" && chunk_size >= 4 && &bytes[chunk_start..chunk_start + 4] == b"INFO"
        {
            parse_wav_info_list(&bytes[chunk_start + 4..chunk_end], &mut metadata)?;
        }

        cursor = chunk_end + (chunk_size % 2);
    }

    Ok(metadata)
}

fn parse_wav_info_list(
    bytes: &[u8],
    metadata: &mut EmbeddedFileMetadata,
) -> Result<(), MetadataError> {
    let mut cursor = 0usize;

    while cursor + 8 <= bytes.len() {
        let chunk_id = &bytes[cursor..cursor + 4];
        let chunk_size = read_u32_le(bytes, cursor + 4)? as usize;
        let value_start = cursor + 8;
        let value_end = value_start
            .checked_add(chunk_size)
            .ok_or(MetadataError::InvalidWav)?;
        if value_end > bytes.len() {
            return Err(MetadataError::InvalidWav);
        }

        let value = decode_info_text(&bytes[value_start..value_end]);
        match chunk_id {
            b"INAM" => metadata.title = value,
            b"IGNR" => metadata.genre = value,
            b"ICMT" => metadata.comment = value,
            _ => {}
        }

        cursor = value_end + (chunk_size % 2);
    }

    Ok(())
}

fn decode_info_text(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes)
        .trim_matches(char::from(0))
        .trim()
        .to_string();

    (!text.is_empty()).then_some(text)
}

fn parse_wav_pcm(bytes: &[u8]) -> Result<DecodedAudioBuffer, MetadataError> {
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(MetadataError::InvalidWav);
    }

    let mut cursor = 12usize;
    let mut format_tag = None;
    let mut channels = None;
    let mut sample_rate = None;
    let mut bits_per_sample = None;
    let mut data = None;

    while cursor + 8 <= bytes.len() {
        let chunk_id = &bytes[cursor..cursor + 4];
        let chunk_size = read_u32_le(bytes, cursor + 4)? as usize;
        let chunk_start = cursor + 8;
        let chunk_end = chunk_start
            .checked_add(chunk_size)
            .ok_or(MetadataError::InvalidWav)?;
        if chunk_end > bytes.len() {
            return Err(MetadataError::InvalidWav);
        }

        match chunk_id {
            b"fmt " => {
                if chunk_size < 16 {
                    return Err(MetadataError::InvalidWav);
                }
                format_tag = Some(read_u16_le(bytes, chunk_start)?);
                channels = Some(read_u16_le(bytes, chunk_start + 2)?);
                sample_rate = Some(read_u32_le(bytes, chunk_start + 4)?);
                bits_per_sample = Some(read_u16_le(bytes, chunk_start + 14)?);
            }
            b"data" => data = Some(&bytes[chunk_start..chunk_end]),
            _ => {}
        }

        cursor = chunk_end + (chunk_size % 2);
    }

    let format_tag = format_tag.ok_or(MetadataError::InvalidWav)?;
    let channels = channels.ok_or(MetadataError::InvalidWav)?;
    let sample_rate = sample_rate.ok_or(MetadataError::InvalidWav)?;
    let bits_per_sample = bits_per_sample.ok_or(MetadataError::InvalidWav)?;
    let data = data.ok_or(MetadataError::InvalidWav)?;

    if format_tag != 1 || channels == 0 || bits_per_sample != 16 || data.len() % 2 != 0 {
        return Err(MetadataError::InvalidWav);
    }

    let samples = data
        .chunks_exact(2)
        .map(|sample| {
            let value = i16::from_le_bytes([sample[0], sample[1]]);
            if value == i16::MIN {
                -1.0
            } else {
                value as f32 / i16::MAX as f32
            }
        })
        .collect();

    Ok(DecodedAudioBuffer {
        sample_rate,
        channels,
        samples,
    })
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Result<u16, MetadataError> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or(MetadataError::InvalidWav)?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32, MetadataError> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or(MetadataError::InvalidWav)?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn mvp_supports_required_formats() {
        for extension in ["wav", "aiff", "mp3", "flac", "m4a", "ogg"] {
            assert!(supported_mvp_format(extension));
        }
    }

    #[test]
    fn extracts_immediate_file_metadata_without_decoding_audio() {
        let mut path = std::env::temp_dir();
        path.push(format!("darkwave-metadata-{}.wav", Uuid::new_v4()));
        fs::write(&path, b"audio fixture").expect("fixture");

        let metadata = extract_immediate_metadata(&path).expect("metadata");

        assert_eq!(metadata.extension, "wav");
        assert_eq!(metadata.file_size, 13);
    }

    #[test]
    fn extracts_wav_info_metadata_fields() {
        let mut path = std::env::temp_dir();
        path.push(format!("darkwave-info-metadata-{}.wav", Uuid::new_v4()));
        fs::write(&path, wav_info_fixture()).expect("fixture");

        let metadata = extract_embedded_metadata(&path).expect("metadata");

        assert_eq!(metadata.title, Some("Dark Metallic Impact".to_string()));
        assert_eq!(metadata.genre, Some("Sound Effects".to_string()));
        assert_eq!(metadata.comment, Some("short trailer hit".to_string()));
    }

    #[test]
    fn decodes_16_bit_wav_pcm_to_normalized_samples() {
        let mut path = std::env::temp_dir();
        path.push(format!("darkwave-decoder-{}.wav", Uuid::new_v4()));
        fs::write(&path, wav_16_bit_fixture()).expect("fixture");

        let decoded = decode_wav_pcm(&path).expect("decode wav");

        assert_eq!(decoded.sample_rate, 48_000);
        assert_eq!(decoded.channels, 1);
        assert_eq!(decoded.samples.len(), 3);
        assert_eq!(decoded.samples[0], -1.0);
        assert_eq!(decoded.samples[1], 0.0);
        assert!(decoded.samples[2] > 0.99);
    }

    #[test]
    fn wav_decoder_rejects_non_wav_extension() {
        let mut path = std::env::temp_dir();
        path.push(format!("darkwave-decoder-{}.mp3", Uuid::new_v4()));
        fs::write(&path, b"not wav").expect("fixture");

        assert_eq!(
            decode_wav_pcm(&path).map(|_| ()),
            Err(MetadataError::UnsupportedDecoderFormat("mp3".to_string()))
        );
    }

    #[test]
    fn supported_audio_decoder_uses_native_wav_without_packaged_decoder() {
        let mut path = std::env::temp_dir();
        path.push(format!("darkwave-decoder-{}.wav", Uuid::new_v4()));
        fs::write(&path, wav_16_bit_fixture()).expect("fixture");

        let decoded = decode_supported_audio(&path, None).expect("decode wav");

        assert_eq!(decoded.sample_rate, 48_000);
        assert_eq!(decoded.channels, 1);
    }

    #[test]
    fn supported_audio_decoder_delegates_packaged_formats() {
        let mut path = std::env::temp_dir();
        path.push(format!("darkwave-decoder-{}.mp3", Uuid::new_v4()));
        fs::write(&path, b"packaged fixture").expect("fixture");
        let decoder = FixturePackagedDecoder;

        let decoded = decode_supported_audio(&path, Some(&decoder)).expect("decode packaged");

        assert_eq!(
            decoded,
            DecodedAudioBuffer {
                sample_rate: 44_100,
                channels: 2,
                samples: vec![0.25, -0.25],
            }
        );
    }

    #[test]
    fn supported_audio_decoder_reports_missing_packaged_decoder() {
        let mut path = std::env::temp_dir();
        path.push(format!("darkwave-decoder-{}.flac", Uuid::new_v4()));
        fs::write(&path, b"packaged fixture").expect("fixture");

        assert_eq!(
            decode_supported_audio(&path, None),
            Err(MetadataError::PackagedDecoderUnavailable(
                "flac".to_string()
            ))
        );
    }

    #[test]
    fn codec_support_marks_wav_native_and_compressed_formats_packaged() {
        assert_eq!(
            codec_support_for_extension("wav"),
            CodecSupport {
                extension: "wav".to_string(),
                status: CodecSupportStatus::NativePcm,
                conversion_available: false,
            }
        );
        assert_eq!(
            codec_support_for_extension("mp3"),
            CodecSupport {
                extension: "mp3".to_string(),
                status: CodecSupportStatus::RequiresPackagedDecoder,
                conversion_available: true,
            }
        );
    }

    #[test]
    fn unsupported_codec_remains_visible_with_conversion_option() {
        assert_eq!(
            codec_support_for_extension("wma"),
            CodecSupport {
                extension: "wma".to_string(),
                status: CodecSupportStatus::Unsupported,
                conversion_available: true,
            }
        );
    }

    fn wav_16_bit_fixture() -> Vec<u8> {
        let samples = [-32768i16, 0, 32767];
        let data_size = samples.len() as u32 * 2;
        let mut wav = Vec::new();

        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_size).to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&48_000u32.to_le_bytes());
        wav.extend_from_slice(&96_000u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_size.to_le_bytes());
        for sample in samples {
            wav.extend_from_slice(&sample.to_le_bytes());
        }

        wav
    }

    fn wav_info_fixture() -> Vec<u8> {
        let mut list_payload = Vec::new();
        list_payload.extend_from_slice(b"INFO");
        append_info_chunk(&mut list_payload, b"INAM", b"Dark Metallic Impact");
        append_info_chunk(&mut list_payload, b"IGNR", b"Sound Effects");
        append_info_chunk(&mut list_payload, b"ICMT", b"short trailer hit");

        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&((4 + 8 + list_payload.len()) as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"LIST");
        wav.extend_from_slice(&(list_payload.len() as u32).to_le_bytes());
        wav.extend_from_slice(&list_payload);
        wav
    }

    fn append_info_chunk(payload: &mut Vec<u8>, id: &[u8; 4], value: &[u8]) {
        payload.extend_from_slice(id);
        payload.extend_from_slice(&(value.len() as u32).to_le_bytes());
        payload.extend_from_slice(value);
        if value.len() % 2 == 1 {
            payload.push(0);
        }
    }

    struct FixturePackagedDecoder;

    impl PackagedAudioDecoder for FixturePackagedDecoder {
        fn decode_packaged_audio(
            &self,
            path: &Path,
            extension: &str,
        ) -> Result<DecodedAudioBuffer, MetadataError> {
            assert_eq!(extension, "mp3");
            assert!(path.exists());
            Ok(DecodedAudioBuffer {
                sample_rate: 44_100,
                channels: 2,
                samples: vec![0.25, -0.25],
            })
        }
    }
}
