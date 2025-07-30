use anyhow::Result;
use hound::{WavSpec, WavWriter};
use tempfile::NamedTempFile;

pub fn save_wav(samples: &[f32], sample_rate: u32, channels: u16) -> Result<NamedTempFile> {
    const MIN_AUDIO_DURATION_MS: u32 = 1000; // 1 second
    let current_duration_ms = (samples.len() as f32 / sample_rate as f32 * 1000.0) as u32;

    let mut padded_samples = samples.to_vec();

    if current_duration_ms < MIN_AUDIO_DURATION_MS {
        let samples_to_add = (sample_rate as f32
            * (MIN_AUDIO_DURATION_MS - current_duration_ms) as f32
            / 1000.0) as usize;
        padded_samples.extend(vec![0.0; samples_to_add]);
        tracing::debug!(
            "Padded audio with {} samples of silence to reach {} ms",
            samples_to_add,
            MIN_AUDIO_DURATION_MS
        );
    }

    let temp_file = NamedTempFile::new()?;
    let mut writer = WavWriter::create(
        temp_file.path(),
        WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )?;

    for &sample in &padded_samples {
        writer.write_sample((sample * i16::MAX as f32) as i16)?;
    }
    writer.finalize()?;
    Ok(temp_file)
}
