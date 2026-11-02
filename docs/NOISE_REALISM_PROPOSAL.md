# Proposal: Realistic Band Noise and Receiver Audio

- **Status:** Initial noise-generator fixes implemented on `improve-noise-realism`; receiver redesign remains a draft
- **Date:** 2026-11-01
- **Baseline:** `master` at `b490ffb` (v0.7.0)
- **Scope:** Initial fixes: `src/audio/noise.rs` and `src/audio/mixer.rs`. Later SNR work also touches `src/config.rs`, `src/ui/settings_panel.rs`, caller generation, and potentially station messages. Receiver processing must consistently use the actual device sample rate.

## 1. Background

Users have reported in person (not on GitHub) that the background noise "isn't realistic at all." This document explains why the current noise sounds wrong and proposes changes in phases. Phases can be reviewed and merged on their own.

File/line references in Sections 2–3 describe the baseline commit, before the changes below. The original measurements were unseeded diagnostic snapshots; they are not exact reproducible benchmarks. Section 7 now provides a seeded probe for the current implementation.

Implemented in this first increment: configured startup filter, QRN entirely before the filter with no sub-audio oscillator output, slow held-target background level variation, and exponentially decaying pops. Configuration fields and stored values are retained. Background audio will sound quieter because broadband hiss and sub-audio energy have been removed. Crashes retain their existing noise-burst model; clustered crackle, shared caller filtering, SNR controls, AGC, and QRM remain future work.

## 2. Baseline audio path (before this branch)

```
NoiseGenerator::next_sample (src/audio/noise.rs:258)
  white -> PinkNoise -> (+ crash + pop) -> 2nd-order bandpass -> * level * floor_mod
                                                                         |
  qrn_sample() (sub-audio sines + UNFILTERED white) * level --------------+--> noise sample

Mixer::fill_buffer (src/audio/mixer.rs:371)
  buffer = noise
  buffer += each caller (pure sine, amplitude 0.4..1.0, NOT filtered)
  buffer += user sidetone
  buffer = soft_clip(buffer * master_volume + dither)
```

Relevant defaults (`src/config.rs:176-198`): `tone_frequency_hz = 600`, `noise_bandwidth = 350`, `noise_level = 0.25`, `master_volume = 0.7`, `crash_rate = 0.4`, `crash_intensity = 0.2`, `pop_rate = 0.6`, `pop_intensity = 0.73`, `qrn_intensity = 0.3`. Caller amplitude defaults are `amplitude_min = 0.4` and `amplitude_max = 1.0` (`src/config.rs:219-220`).

## 3. Findings

Historical, approximate measurements of baseline `NoiseGenerator` output at 44.1 kHz with `level = 0.25` and the filter set to 600 Hz / 350 Hz. RMS used 20 s; the supplied original spectral probe used only the first 5 s. These numbers are retained as diagnostic context, not acceptance thresholds for the new implementation.

| Signal | RMS |
|---|---|
| Filtered band noise only (all effects off) | 0.0063 |
| QRN component alone (`qrn_intensity = 0.3`) | 0.0276 |
| Band noise + QRN 0.3 | 0.0283 |
| Full defaults | 0.0283 |

Spectrum in dB (relative, Hann-windowed DFT averaged over 4096-sample frames):

| Freq (Hz) | Band noise only | Band noise + QRN | Full defaults |
|---:|---:|---:|---:|
| 20 | -13.4 | 4.5 | 4.5 |
| 100 | -8.5 | -4.4 | -4.6 |
| 300 | -0.3 | 0.1 | 1.0 |
| 600 | 3.6 | 4.3 | 4.7 |
| 1000 | -4.6 | -2.5 | -2.8 |
| 2000 | -14.9 | -7.1 | -8.9 |
| 5000 | -26.9 | -8.1 | -7.1 |
| 12000 | -42.5 | -9.3 | -9.1 |
| 18000 | -52.9 | -8.5 | -7.8 |

### F1. QRN adds unfiltered broadband hiss and sub-audio wobble (highest impact)

- `qrn_sample()` (`src/audio/noise.rs:228-255`) is added **after** the bandpass filter (`:282-284`).
- It contains:
  - 0.3 Hz and 0.51 Hz sine waves, far below hearing. They only move the waveform's DC level; peak 0.084.
  - Uniform white noise of ±0.3 that is never filtered, so it is flat to Nyquist.
- Total QRN RMS is about 13 dB above filtered-background RMS in this snapshot, but includes substantial inaudible sub-audio energy. That ratio cannot establish audible dominance. The relatively flat high-frequency spectrum does independently reveal the unfiltered hiss path; perceived realism still needs listening comparisons.
- `mod_depth` uses a third sine at 0.03 Hz (about 33 s per cycle), so the hiss swells and fades over roughly half a minute. Measured 100 ms RMS ranges from 0.0083 to 0.062.

### F2. Signal-to-noise ratio is unrealistically high

- Callers are full-scale sine waves (`src/audio/morse.rs:75`) scaled by 0.4–1.0 (`src/audio/mixer.rs:169`).
- Signal RMS is 0.28–0.71, against filtered noise of 0.0063. **SNR in the 350 Hz passband is about 33–41 dB.** Even at the slider maximum `noise_level = 0.5` (`src/ui/settings_panel.rs:325`) it is still about 27–35 dB.
- These are nominal key-down estimates without QSB; they do not establish that noise can never mask a signal. A useful weak-signal practice preset may include 0–15 dB reference SNR, but defaults need listening review using an explicitly defined reference bandwidth.

### F3. Receiver filter is too gentle

- A single RBJ 2nd-order bandpass (`src/audio/noise.rs:42-63`). Q = 600/350 ≈ 1.7.
- Calculated from the actual filter coefficients at 44.1 kHz: about 8.8 dB down at 300 Hz, 6.4 dB at 1 kHz, and 23.4 dB at 5 kHz, relative to 600 Hz. The colored-noise spectrum above is not the filter transfer function.
- A steeper receiver model may better reproduce the selectivity and ringing of a narrow CW receiver. Define its passband and stopband requirements before selecting an order or topology, then compare its sound with recordings.
- The `PinkNoise` stage (`:80-104`) makes almost no difference inside a passband this narrow.

### F4. Callers bypass the receiver filter

- Only noise goes through the filter. Callers are added unfiltered (`src/audio/mixer.rs:395-408`).
- Narrowing `noise_bandwidth` changes only the noise. A caller well outside the passband is still heard at full strength. With a real receiver, narrowing the filter is a tool for pulling out one caller from a pileup.

### F5. No AGC

- Receiver gain can decrease under strong signals and static crashes, then recover according to its threshold, attack, hang, and release behavior. Pronounced pumping between characters is not a universal realism requirement; avoid introducing exaggerated breathing as an acceptance target.
- The current mixer sums sources at fixed levels and only has a soft clipper at 0.8 (`src/audio/mixer.rs:440-450`).

### F6. Noise-floor modulation is effectively zero

- `src/audio/noise.rs:274-277` picks a new uniform random target **every sample** and passes it through a 0.3 Hz one-pole low-pass filter. The random targets average out. Measured peak `|noise_floor|` = 0.007, so `floor_mod` stays within 1 ± 0.0008 at QRN 0.3. The feature does nothing.

### F7. Pops are square pulses, not impulses

- `pop_sample()` (`src/audio/noise.rs:217-226`) computes `progress = 1 - min(remaining/5, 1)`. That is 0 until the last 5 samples, so a 1–5 ms pop is a **flat DC step** that ramps down only at the end. After the bandpass this sounds like two clicks, one at each edge, not a single tick.
- The doc comment says "sharp attack, quick decay," which doesn't match the code.

### F8. Crashes are uniform noise bursts

- `maybe_start_crash()` and `crash_sample()` (`:169-214`) produce one burst of uniform white noise with exponential decay (50–200 ms).
- A clustered impulsive model is a candidate for a more varied crash sound. Its distribution, timing, and level need listening comparison; a particular synthetic distribution is not established as the only realistic choice.

### F9. Configured filter not applied at startup

- `NoiseGenerator::new` hard-codes 600 Hz / 400 Hz (`src/audio/noise.rs:138`). `Mixer::new` (`src/audio/mixer.rs:322-329`) doesn't apply settings. `update_filter` is only called from `Mixer::update_settings` (`:359-361`), which runs only when an `AudioCommand::UpdateSettings` arrives (`src/audio/engine.rs:137-138`).
- Users with a non-default tone or bandwidth get the wrong filter until they change a setting.

## 4. Proposed changes

Begin with the confirmed generator defects in Phase 1. Implement and validate the shared receiver filter from Phase 3a before final Phase 2 SNR calibration. Introduce AGC as a separate increment so its effect can be assessed. Only the initial fixes stay within `noise.rs` and `mixer.rs`; Phase 2 requires configuration, UI, and caller-strength changes.

### Phase 1: Fix the noise generator (F1, F6, F7, F9; F8 remains proposed)

**1a. Apply configured filter at construction (F9).**
Change the signature to `NoiseGenerator::new(sample_rate, center_hz, bandwidth_hz)` and pass `settings.tone_frequency_hz` and `settings.noise_bandwidth` from `Mixer::new`.

**1b. Rewrite QRN (F1, F6).**
- Remove the sine oscillators and the unfiltered white term.
- The initial implementation uses filtered broadband QRN excitation plus held-target level drift. A later crackle refinement can split the excitation into these parts, all injected **before** the filter:
  - **Level drift:** a smoothed random process at 0.1–1 Hz controlling the band-noise gain. Implement it as a held random target that changes at a low rate (e.g. a new target every 200–2000 ms, chosen at random), followed by a one-pole low-pass filter. That fixes F6. Depth in dB scales with `qrn_intensity`, e.g. ±`6 * qrn_intensity` dB.
  - **Crackle:** a Poisson stream of small impulses whose rate scales with `qrn_intensity` (e.g. 0–150 per second). Amplitudes come from a heavy-tailed distribution (e.g. Pareto or log-normal) and are capped to avoid runaway peaks. Through the filter, these become the characteristic atmospheric crackle.
- Reuse the existing smoother with a held target and sample-counted hold duration. Do not emit the slow modulation itself as an audio signal.

**1c. Pops as true impulses (F7).**
Implemented: a random-polarity exponential excitation with a 0.2–0.5 ms time constant and an eight-time-constant tail. Timing is expressed in seconds, and filtered event energy is checked at 44.1/48 kHz. A fixed-amplitude single-sample replacement would lose energy and depend on sample rate; it would require explicit normalization. The filter may produce multiple oscillations from one excitation.

**1d. Future refinement: crashes as impulse clusters (F8).**
A crash event lasting 100–600 ms with an envelope (attack under 5 ms, exponential decay). Inside it, a dense Poisson stream of heavy-tailed impulses (e.g. 500–3000 per second) scaled by the envelope and `crash_intensity`.

**1e. Make tests possible.**
Implemented: test-only `NoiseGenerator::with_seed(sample_rate, center_hz, bandwidth_hz, seed)` so tests and diagnostic probes are deterministic.

**Acceptance criteria (automated tests):**
- At the generator output, before mixer dither/clipping, seeded all-effects output at 600 Hz / 350 Hz must have at least 15 dB less power at 5 kHz than at 600 Hz, averaged over 10 s after 1 s warm-up, at both 44.1/48 kHz. The retained filter provides about 23.4 dB rejection of white excitation there; a guaranteed 30 dB target belongs with a stronger filter. This regression rejects the former unfiltered QRN path.
- For the same seeded render, mean amplitude over 10 s is within 0.0001 of zero and power at 20 Hz is at least 20 dB below center. This detects low-frequency leakage but is not a direct spectral measurement of 0.3 Hz. Slow variation is a gain envelope, not an additive oscillator.
- With `qrn_intensity = 1.0`, the spread of 1-second RMS values is well above the spread at `qrn_intensity = 0` (e.g. coefficient of variation more than 3× higher).
- A pop excitation decays immediately rather than holding a rectangular plateau; its filtered energy is comparable across 44.1/48 kHz. Ringing in the filtered waveform is expected. Do not require a single waveform peak.
- A mixer constructed at 1 kHz / 100 Hz applies that noise passband before any settings update, verified by a seeded spectral comparison against the old hard-coded 600 Hz center. Seeded rendering must also be identical across different buffer partitions.

### Phase 2: Calibrate signal-to-noise ratio (F2)

**2a. Calibrate the noise reference.**
Define a fixed stationary-noise reference RMS in a specified 500 Hz receiver response. State the response and equivalent noise bandwidth, rather than assuming nominal bandwidth alone determines integrated noise. Caller reference strength is measured during on-frequency key-down, before QSB, AGC, dither, and clipping, with impulsive effects and drift disabled. Narrowing the actual receiver filter should reduce integrated noise without rescaling the caller.

**2b. Express caller strength as SNR.**
Add `snr_min_db` and `snr_max_db` to `SimulationSettings` using serde defaults. Initially evaluate an easier 10–35 dB reference range, with a harder 3–30 dB preset. If reference noise RMS is sigma, sine peak amplitude is `sqrt(2) * sigma * 10^(snr_db/20)`. Keep this amplitude independent of the live noise slider and current bandwidth: increasing noise should reduce actual SNR, and muting noise must not mute callers. Preserve each caller's selected strength across repeats. Legacy conversion requires explicit version/presence handling; see Section 6.

**2c. Rescale the UI.**
- The noise slider should cover a meaningful range, e.g. -20 to +10 dB around the reference, labeled in dB.
- The caller strength sliders should be labeled as SNR in dB.

**2d. Check headroom.**
Check all five strongest simultaneous callers plus noise and capped impulsive events. Define a peak ceiling and allowed clipping behavior before finalizing levels. AGC attack is finite and does not by itself prevent initial peak overload.

**Acceptance criteria:**
- With the exact reference filter and noise level, on-frequency key-down SNR measures within ±1 dB of X before AGC. At other bandwidths/noise settings, test the expected change in actual SNR rather than forcing it back to X. Test noise-off behavior independently.
- At default settings, the weakest callers are clearly near the noise level when listening (see §5).

### Phase 3: Receiver chain with shared filter and AGC (F3, F4, F5)

**3a. Shared receiver filter.**
Move the filter out of `NoiseGenerator` into `Mixer`. Apply it to **noise + callers**, not to the user sidetone.
- First specify passband loss, stopband rejection, and transition width. Choose a true bandpass design implemented as second-order sections and validate the response. Independently cascading a fourth-order high-pass at 425 Hz and fourth-order low-pass at 775 Hz gives only about 7.1 dB rejection at 950 Hz relative to 600 Hz, failing the 20 dB criterion below. It is not equivalent to a Butterworth bandpass transformation.
- Define valid cutoffs: `0 < low < high < sample_rate/2`. Current controls allow a 400 Hz center with 1,000 Hz bandwidth, so blindly using `f0 - BW/2` produces a negative cutoff. Constrain or explicitly map such combinations. Normalize in-band gain and define how settings changes preserve state or crossfade without clicks.
- Remove the old generator filter when the shared filter takes over; avoid filtering noise twice.
- Optional "filter shape" setting (soft / normal / sharp) to choose the order.
- Watch for stability at narrow bandwidths with `f32`. Consider computing the coefficients in `f64` or running the filter state in `f64`.

**3b. AGC.**
After the filter, specify detector type (peak or RMS), threshold, gain limits, initial gain, and smoothing. Candidate timing is 2–5 ms attack and 150–400 ms release with explicit hang behavior; validate it by listening. Define behavior below threshold so ambient noise is not continuously boosted to the target, and define recovery after long TX/muted-RX periods. The simple `target / max(env, floor)` expression alone does not specify these behaviors.
- Add a setting `agc: { enabled, release_ms }` with serde defaults.
- Strong callers and crashes may reduce receiver gain, followed by controlled recovery. Avoid audible gain steps, excessive noise breathing, and unstable gain during silence; test attack overshoot separately from the release envelope.

**3c. Restructure the mixer.**
Render RX (noise + callers) into a reusable scratch buffer, filter it, apply AGC, then add the sidetone, master volume, dither and clipper. Allocate scratch storage outside the callback or reuse existing capacity. Apply RX muting at a defined stage and state whether filter/noise/AGC clocks advance while muted. Preserve caller and user segment completion events independently of audible muting.

**Acceptance criteria:**
- With AGC disabled, a caller offset by `+BW` or `-BW` from center is attenuated by at least 20 dB compared with an on-frequency caller, for valid positive audio frequencies. Specify passband ripple/loss as well. Perform filter measurements before dither and clipping.
- With AGC on, measure gain reduction for a strong caller and bounded recovery after it ends, against the chosen timing contract. Do not require pronounced recovery between every character.
- Add explicit tests for sidetone level/timing, segment completion order, station completion during muted RX, TX/RX transitions, buffer partitioning, and filter changes. The baseline has Morse tests but no segment-completion tests to rely on.

**Behavior note:** caller offsets are drawn from `-frequency_spread_hz/2 .. frequency_spread_hz/2`, so the default 300 Hz spread means offsets of -150 to +150 Hz, not +/-300 Hz. Measure the chosen filter loss there before adjusting difficulty defaults.

### Phase 4 (optional): Background QRM

The noise on a busy contest band is largely other signals: distant CQs, partial exchanges, and stations near the edge of the filter.
- Add a QRM source of non-interactive CW stations, started at random times. Each sends short CQ or exchange fragments at random offsets (inside and outside the passband) at low SNR (e.g. -5 to +10 dB). They go through the Phase 3 filter.
- Implement them as `ActiveStation` instances marked so their completion is not reported in `completed_stations`. That keeps the caller and QSO logic unchanged.
- Settings: `qrm: { enabled, density, snr_min_db, snr_max_db }`.

## 5. Validation plan

1. **Automated:** use seeded generators, record all parameters, discard a specified warm-up period, and report the measured stage and spectral normalization. Exercise 44.1/48 kHz, buffer boundaries, filter extremes, and mute/settings transitions. The Section 7 probe is diagnostic, not a listening-quality assertion.
2. **Listening review:** save ~30 s WAV clips (debug-only, or a test that writes into `target/`) at defaults and at "heavy QRN" for each phase. Have experienced CW contesters compare them, ideally next to real off-air recordings of a contest band through a 400–500 Hz filter. Checklist:
   - Has the unfiltered high-frequency hiss been removed without introducing audible transients? Some stopband energy and post-filter output dither remain; do not require mathematical zero above 2 kHz.
   - Does narrowing the bandwidth make the noise sound narrower and more hollow, with ringing?
   - Are weak callers genuinely hard to copy at the low end of the default SNR range?
   - Do crashes sound like lightning, and does the AGC pump and recover believably?
3. **Performance:** an 8th-order filter plus AGC at 48 kHz is cheap, but confirm there are no audio dropouts on the slowest supported machine.

## 6. Compatibility and migration

- The initial fixes add no settings fields and retain saved values; no config reset or migration is performed. Document the changed sound caused by removing unfiltered QRN and correcting pops.
- Later new settings fields must support old files, but serde defaults alone cannot decide whether legacy amplitudes require conversion. Use an explicit version or detect field presence before applying defaults. Specify precedence if both amplitude and SNR settings exist, convert only once, and preserve the original configuration. Do not overwrite an intentional new SNR value with a legacy conversion.
- The *meaning* of `qrn_intensity`, `crash_intensity` and `noise_level` changes, so existing saved values will sound different. Either bump a config version and reset the noise section to the new defaults once, or accept the change and note it in the release notes. Needs a decision (see §8).
- Difficulty will change noticeably: lower SNR in Phase 2, filtered callers in Phase 3. Consider a short release note telling users how to restore an "easy" setup.

## 7. Appendix: reproducible measurement report

The committed diagnostic test replaces the incomplete, unseeded temporary probe that produced the historical snapshot in Section 3. It reports the **current** implementation and is not expected to reproduce the old numbers exactly.

```sh
cargo test --locked audio::noise::tests::report_noise_spectrum -- --ignored --nocapture
```

The report runs four named cases: background only, background plus QRN at 0.3, full defaults, and heavy effects (crash rate 5/s, pop rate 10/s, all intensities 1.0). Every case uses seed 42, 600 Hz center, 350 Hz bandwidth, and output level 0.25, at both 44.1 and 48 kHz. Each render discards 1 s of warm-up, then measures 20 s of generator output, before mixer dither, clipping, sidetone, or callers.

Total RMS and mean are time-domain measurements of that output. Spectral values average Hann-windowed 4096-sample DFT power over all complete frames, expressed in dB relative to the same render's 600 Hz power. These are output spectra, not dBFS, isolated component powers, or filter transfer-function measurements. In particular, total RMS must not be interpreted as audible-band RMS; listening comparisons and a separately defined audible-band measurement are still needed for perceptual claims. A dedicated low-frequency analysis with a sufficiently long window would be required to resolve sub-audio oscillations.

The regular regression suite separately checks stopband leakage, mean offset, slow RMS variation, immediate pop decay and cross-rate energy, startup filter configuration, and buffer partition independence. The diagnostic report is ignored by default so reporting does not add work or depend on exact printed random values in CI.

## 8. Open questions for reviewers

1. **Default SNR range:** is 3–30 dB right for the target audience, or should the default lean easier (e.g. 10–35 dB), with "contest realistic" as a preset?
2. **Filtering callers (Phase 3a):** should this be always on, or behind a setting at first, since it changes difficulty?
3. **Settings migration:** reset the noise settings once for existing users, or keep their values under the new meaning?
4. **Legacy amplitude fields:** convert `amplitude_min` and `amplitude_max` to SNR automatically, or deprecate them and switch to new defaults?
5. **Phase 4 (QRM):** in scope for this effort, or a separate issue?
6. **Reference recordings:** do we have, or can we get, licensed off-air contest recordings to use as a listening benchmark?
