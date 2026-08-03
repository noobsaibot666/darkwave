use std::fs::File;
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::{DecodedAudioBuffer, MetadataError, PackagedAudioDecoder};

/// Decodes any format this crate's Cargo.toml enables Symphonia support for
/// (mp3, flac, ogg/vorbis, aiff — deliberately not aac/m4a, see the
/// Cargo.toml comment on the symphonia dependency) into normalized f32 PCM.
/// Plugs into the `PackagedAudioDecoder` seam that `decode_supported_audio`
/// has always had but nothing implemented.
pub struct SymphoniaDecoder;

impl PackagedAudioDecoder for SymphoniaDecoder {
    fn decode_packaged_audio(
        &self,
        path: &Path,
        extension: &str,
    ) -> Result<DecodedAudioBuffer, MetadataError> {
        let file = File::open(path)?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        hint.with_extension(extension);

        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|error| decode_failed(extension, error))?;

        let mut format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| MetadataError::DecodeFailed(format!("{extension}: no decodable track found")))?
            .clone();

        let track_id = track.id;
        let sample_rate = track
            .codec_params
            .sample_rate
            .ok_or_else(|| MetadataError::DecodeFailed(format!("{extension}: unknown sample rate")))?;
        // Some containers (notably certain AAC-in-MP4 tracks) don't surface
        // the channel count in the track's upfront codec params — it's only
        // known once a packet is actually decoded and its own spec is
        // read. Use the eager value when available (the common, fast case
        // for wav/aiff/flac/mp3) and otherwise fall back to the first
        // successfully decoded packet's spec below, rather than failing a
        // file whose channel count is perfectly determinable, just not yet.
        let mut channels = track.codec_params.channels.map(|channels| channels.count() as u16);

        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|error| decode_failed(extension, error))?;

        let mut samples = Vec::new();

        loop {
            let packet = match format.next_packet() {
                Ok(packet) => packet,
                Err(SymphoniaError::IoError(io_error))
                    if io_error.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break
                }
                Err(SymphoniaError::ResetRequired) => break,
                Err(error) => return Err(decode_failed(extension, error)),
            };

            if packet.track_id() != track_id {
                continue;
            }

            match decoder.decode(&packet) {
                Ok(decoded) => {
                    if channels.is_none() {
                        channels = Some(decoded.spec().channels.count() as u16);
                    }
                    let mut sample_buffer =
                        SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
                    sample_buffer.copy_interleaved_ref(decoded);
                    samples.extend_from_slice(sample_buffer.samples());
                }
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(error) => return Err(decode_failed(extension, error)),
            }
        }

        if samples.is_empty() {
            return Err(MetadataError::DecodeFailed(format!(
                "{extension}: decoded zero samples"
            )));
        }
        let channels = channels
            .ok_or_else(|| MetadataError::DecodeFailed(format!("{extension}: unknown channel count")))?;

        Ok(DecodedAudioBuffer {
            sample_rate,
            channels,
            samples,
        })
    }
}

fn decode_failed(extension: &str, error: impl std::fmt::Display) -> MetadataError {
    MetadataError::DecodeFailed(format!("{extension}: {error}"))
}
