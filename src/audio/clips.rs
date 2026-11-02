//! Listening clips for reviewing noise and receiver changes by ear.
//!
//! Renders seeded scenarios through the real `Mixer` (noise, callers, master
//! volume, dither and clipping, as heard in the app) and writes 16-bit mono
//! WAV files plus a parameter manifest:
//!
//! ```sh
//! cargo test --locked listening_clips -- --ignored --nocapture
//! ```
//!
//! Output goes to `$CARGO_TARGET_DIR/listening-clips/` (default
//! `target/listening-clips/`). Noise is seeded, so clips are reproducible
//! apart from the mixer's tiny output dither.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::audio::mixer::Mixer;
use crate::audio::noise::NoiseGenerator;
use crate::config::{AudioSettings, NoiseSettings};
use crate::contest::Exchange;
use crate::messages::{StationId, StationParams};

const SAMPLE_RATE: u32 = 48_000;
const SECONDS: f32 = 30.0;
const SEED: u64 = 42;
/// Render block size, similar to a real audio callback.
const BLOCK: usize = 512;

/// One transmission by a caller in a scenario's script.
struct Call {
    at_seconds: f32,
    callsign: &'static str,
    message: &'static str,
    offset_hz: f32,
    wpm: u8,
    amplitude: f32,
}

const fn call(
    at_seconds: f32,
    callsign: &'static str,
    message: &'static str,
    offset_hz: f32,
    wpm: u8,
    amplitude: f32,
) -> Call {
    Call {
        at_seconds,
        callsign,
        message,
        offset_hz,
        wpm,
        amplitude,
    }
}

/// A small, repeating pileup: several callers answer a CQ at once, spanning
/// the default caller amplitude range (0.4–1.0) and frequency spread
/// (±150 Hz), then one sends an exchange.
const PILEUP: &[Call] = &[
    call(1.0, "W1AW", "W1AW", -120.0, 26, 0.4),
    call(1.1, "N5DX", "N5DX", 135.0, 24, 1.0),
    call(1.3, "K3LR", "K3LR", 40.0, 30, 0.7),
    call(5.0, "K3LR", "5NN 05", 40.0, 30, 0.7),
    call(9.0, "VE3EJ", "VE3EJ", -60.0, 28, 0.55),
    call(9.2, "W9RE", "W9RE", 100.0, 32, 0.4),
    call(9.4, "KC1XX", "KC1XX", -140.0, 25, 0.85),
    call(13.5, "W9RE", "W9RE", 100.0, 32, 0.4),
    call(17.0, "W9RE", "5NN 04", 100.0, 32, 0.4),
    call(21.0, "AA3B", "AA3B", 10.0, 27, 0.5),
    call(21.1, "K1TO", "K1TO", -90.0, 29, 0.45),
    call(25.0, "K1TO", "K1TO 5NN 03", -90.0, 29, 0.45),
];

struct Scenario {
    name: &'static str,
    description: &'static str,
    settings: AudioSettings,
    calls: &'static [Call],
}

fn base_settings() -> AudioSettings {
    AudioSettings {
        sample_rate: SAMPLE_RATE,
        ..AudioSettings::default()
    }
}

/// Every noise effect at its settings-panel maximum.
fn max_noise() -> NoiseSettings {
    NoiseSettings {
        crash_rate: 2.0,
        crash_intensity: 1.0,
        pop_rate: 10.0,
        pop_intensity: 1.0,
        qrn_intensity: 1.0,
    }
}

fn scenarios() -> Vec<Scenario> {
    let quiet_noise = NoiseSettings {
        crash_rate: 0.0,
        crash_intensity: 0.0,
        pop_rate: 0.0,
        pop_intensity: 0.0,
        qrn_intensity: 0.0,
    };
    vec![
        Scenario {
            name: "01-noise-defaults",
            description: "Band noise only, default settings",
            settings: base_settings(),
            calls: &[],
        },
        Scenario {
            name: "02-noise-background-only",
            description: "Band noise only, all effects (QRN, crashes, pops) off",
            settings: AudioSettings {
                noise: quiet_noise,
                ..base_settings()
            },
            calls: &[],
        },
        Scenario {
            name: "03-noise-max-effects",
            description: "Band noise only, every noise effect at its slider maximum",
            settings: AudioSettings {
                noise: max_noise(),
                ..base_settings()
            },
            calls: &[],
        },
        Scenario {
            name: "04-pileup-defaults",
            description: "Pileup at default settings",
            settings: base_settings(),
            calls: PILEUP,
        },
        Scenario {
            name: "05-pileup-max-effects",
            description: "Pileup with every noise effect at its slider maximum",
            settings: AudioSettings {
                noise: max_noise(),
                ..base_settings()
            },
            calls: PILEUP,
        },
        Scenario {
            name: "06-pileup-max-noise-level",
            description: "Pileup with noise level at its slider maximum (0.5), default effects",
            settings: AudioSettings {
                noise_level: 0.5,
                ..base_settings()
            },
            calls: PILEUP,
        },
        Scenario {
            name: "07-pileup-narrow-200hz",
            description: "Pileup with a 200 Hz receiver bandwidth, default effects",
            settings: AudioSettings {
                noise_bandwidth: 200.0,
                ..base_settings()
            },
            calls: PILEUP,
        },
    ]
}

fn render(scenario: &Scenario) -> Vec<f32> {
    let settings = scenario.settings.clone();
    let mut mixer = Mixer::new(SAMPLE_RATE, settings.clone());
    mixer.noise = NoiseGenerator::with_seed(
        SAMPLE_RATE,
        settings.tone_frequency_hz,
        settings.noise_bandwidth,
        SEED,
    );

    let total = (SECONDS * SAMPLE_RATE as f32) as usize;
    let mut output = vec![0.0; total];
    let mut next_call = 0;
    let mut id = 0;
    for (index, block) in output.chunks_mut(BLOCK).enumerate() {
        let block_start = (index * BLOCK) as f32 / SAMPLE_RATE as f32;
        while let Some(call) = scenario.calls.get(next_call) {
            if call.at_seconds > block_start {
                break;
            }
            id += 1;
            let params = StationParams {
                id: StationId(id),
                callsign: call.callsign.to_string(),
                exchange: Exchange::new(vec!["5NN".to_string()]),
                frequency_offset_hz: call.offset_hz,
                wpm: call.wpm,
                amplitude: call.amplitude,
                reaction_delay_ms: 0,
            };
            mixer.add_station(&params, call.message);
            next_call += 1;
        }
        mixer.fill_buffer(block);
    }
    output
}

fn write_wav(path: &Path, samples: &[f32]) -> std::io::Result<()> {
    let data_len = (samples.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // byte rate
    bytes.extend_from_slice(&2u16.to_le_bytes()); // block align
    bytes.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for &sample in samples {
        let pcm = (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
        bytes.extend_from_slice(&pcm.to_le_bytes());
    }
    std::fs::write(path, bytes)
}

fn output_dir() -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target"));
    target.join("listening-clips")
}

fn git_revision() -> String {
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
    };
    let revision = run(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".into());
    match run(&["status", "--porcelain"]) {
        Some(status) if !status.is_empty() => format!("{revision} (with uncommitted changes)"),
        _ => revision,
    }
}

fn peak_and_rms(samples: &[f32]) -> (f32, f64) {
    let peak = samples.iter().fold(0.0_f32, |peak, &x| peak.max(x.abs()));
    let rms =
        (samples.iter().map(|&x| (x as f64).powi(2)).sum::<f64>() / samples.len() as f64).sqrt();
    (peak, rms)
}

#[test]
fn pileup_script_is_in_time_order() {
    assert!(PILEUP
        .windows(2)
        .all(|w| w[0].at_seconds <= w[1].at_seconds));
    assert!(PILEUP.iter().all(|c| c.at_seconds < SECONDS));
}

#[test]
fn wav_header_is_valid_pcm() {
    let path = std::env::temp_dir().join(format!("contest-trainer-wav-{}.wav", std::process::id()));
    write_wav(&path, &[0.0, 1.0, -1.0, 2.0]).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(bytes.len(), 44 + 8);
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 36 + 8);
    assert_eq!(&bytes[36..40], b"data");
    let pcm: Vec<i16> = bytes[44..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&b| i16::from_le_bytes(b))
        .collect();
    assert_eq!(pcm, [0, i16::MAX, -i16::MAX, i16::MAX]);
}

#[test]
#[ignore = "writes WAV files for listening review; run explicitly with --ignored"]
fn listening_clips() {
    let dir = output_dir();
    std::fs::create_dir_all(&dir).unwrap();

    let mut manifest = String::new();
    writeln!(manifest, "Listening clips").unwrap();
    writeln!(manifest, "revision: {}", git_revision()).unwrap();
    writeln!(
        manifest,
        "{SAMPLE_RATE} Hz, 16-bit mono, {SECONDS} s, noise seed {SEED}, rendered through Mixer\n"
    )
    .unwrap();

    for scenario in scenarios() {
        let samples = render(&scenario);
        let (peak, rms) = peak_and_rms(&samples);
        let path = dir.join(format!("{}.wav", scenario.name));
        write_wav(&path, &samples).unwrap();
        println!("{} (peak {peak:.3}, RMS {rms:.4})", path.display());

        let s = &scenario.settings;
        writeln!(manifest, "{}.wav: {}", scenario.name, scenario.description).unwrap();
        writeln!(
            manifest,
            "  tone {} Hz, bandwidth {} Hz, noise level {}, master volume {}",
            s.tone_frequency_hz, s.noise_bandwidth, s.noise_level, s.master_volume
        )
        .unwrap();
        writeln!(manifest, "  noise {:?}", s.noise).unwrap();
        writeln!(manifest, "  qsb {:?}", s.qsb).unwrap();
        writeln!(manifest, "  callers: {}", scenario.calls.len()).unwrap();
        writeln!(manifest, "  output peak {peak:.3}, RMS {rms:.4}\n").unwrap();
    }
    writeln!(
        manifest,
        "Pileup script (time s, call, message, offset Hz, wpm, amplitude):"
    )
    .unwrap();
    for c in PILEUP {
        writeln!(
            manifest,
            "  {:5.1}  {:6} {:12} {:+6.0} {:3} {:.2}",
            c.at_seconds, c.callsign, c.message, c.offset_hz, c.wpm, c.amplitude
        )
        .unwrap();
    }
    let manifest_path = dir.join("clips.txt");
    std::fs::write(&manifest_path, manifest).unwrap();
    println!("{}", manifest_path.display());
}
