# Proposal: Realistic Band Noise and Receiver Audio

- **Status:** Phase 1 (noise-generator fixes, `9467a9b`) and its follow-ups (§4.2) implemented on `improve-noise-realism`. Phases 2–6 are planned (see §5).
- **Delivery:** all phases land on `improve-noise-realism` and ship as one release. Nothing merges to `master` until the work is complete.
- **Last updated:** 2026-11-02 (first draft 2026-11-01)
- **Baseline:** `master` at `b490ffb` (v0.7.0)
- **Scope:** Phase 1 touched only `src/audio/noise.rs` and `src/audio/mixer.rs`. Later phases also touch `src/config.rs`, `src/ui/settings_panel.rs`, caller generation, and possibly station messages. All receiver processing must use the actual device sample rate.

**Contents:** §1 Background · §2 Status at a glance · §3 Baseline findings · §4 Completed work · §5 Remaining work (in execution order) · §6 Validation plan · §7 Compatibility and migration · §8 Decisions needed · §9 Appendix: measurements

## 1. Background

Users have reported in person (not on GitHub) that the background noise "isn't realistic at all." This document explains why the original noise sounded wrong, records what has been fixed, and plans the remaining changes in phases.

This is one cohesive body of work. Phases are milestones on a single branch, each landing as one or more commits with its own tests, so that intermediate states are reviewable. They are not separate releases or PRs. Intermediate states (for example, Phase 1 without Phase 2's crackle) never reach users, so release notes and settings migration are written once, for the final result.

**Phase numbering changed on 2026-11-02.** Phases are now numbered in the order the work must happen. The shared receiver filter must exist before SNR calibration can be defined against it. Old → new mapping: old 1b crackle and 1d crash clusters → Phase 2; old 3a/3c → Phase 3; old Phase 2 → Phase 4; old 3b → Phase 5; old Phase 4 → Phase 6.

## 2. Status at a glance

| Phase | Work | Findings | Status |
|---|---|---|---|
| 1 | Fix noise generator defects | F1, F6, F7, F9 | **Done** (`9467a9b`) |
| 1-FU | Test robustness, threshold alignment, sample-rate normalization | §4.2 | **Done** |
| 2 | QRN character: crackle stream, clustered crashes | F1 (remainder), F8 | Next |
| 3 | Shared receiver filter for noise and callers; mixer restructure | F3, F4 | Planned; needs D5, D6 |
| 4 | Calibrate SNR; caller strength in dB | F2 | Planned; needs D2–D4, D8; depends on Phase 3 |
| 5 | AGC | F5 | Planned |
| 6 | Background QRM (optional) | — | Needs D9 (in or out of this release) |

## 3. Baseline findings (before this branch)

File and line references in this section point to the baseline commit `b490ffb`. The measurements are historical, unseeded snapshots; they are context, not acceptance thresholds. Current seeded numbers are in §9.2.

### 3.1 Baseline audio path

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

### 3.2 Historical measurements

Baseline `NoiseGenerator` output at 44.1 kHz with `level = 0.25` and the filter set to 600 Hz / 350 Hz. RMS used 20 s; the spectral probe used only the first 5 s.

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

### 3.3 Findings

**F1. QRN added unfiltered broadband hiss and sub-audio wobble (highest impact).** *Fixed in Phase 1; distinctive QRN character remains for Phase 2.*
- `qrn_sample()` (`noise.rs:228-255`) was added **after** the bandpass filter (`:282-284`).
- It contained 0.3 Hz and 0.51 Hz sine waves (below hearing; they only shifted the waveform's DC level, peak 0.084), plus uniform white noise of ±0.3 that was never filtered, so it was flat to Nyquist.
- Total QRN RMS was about 13 dB above the filtered background, but much of that was inaudible sub-audio energy. The flat high-frequency spectrum independently shows the unfiltered hiss path.
- `mod_depth` used a third sine at 0.03 Hz (about 33 s per cycle), so the hiss swelled and faded over about half a minute. 100 ms RMS ranged from 0.0083 to 0.062.

**F2. Signal-to-noise ratio is unrealistically high.** *Open, Phase 4.*
- Callers are full-scale sine waves (`src/audio/morse.rs:75`) scaled by 0.4–1.0 (`mixer.rs:169`).
- Signal RMS is 0.28–0.71 against filtered noise of 0.0063: **about 33–41 dB SNR in the 350 Hz passband**. Even at the slider maximum `noise_level = 0.5` (`src/ui/settings_panel.rs:325`) it is about 27–35 dB.
- These are nominal key-down estimates without QSB. A weak-signal practice preset may want 0–15 dB reference SNR; defaults need listening review against an explicitly defined reference bandwidth.

**F3. The receiver filter is too gentle.** *Open, Phase 3.*
- A single RBJ 2nd-order bandpass (`noise.rs:42-63`), Q = 600/350 ≈ 1.7.
- From the actual coefficients at 44.1 kHz, relative to 600 Hz: about 8.8 dB down at 300 Hz, 6.4 dB at 1 kHz, 23.4 dB at 5 kHz.
- The `PinkNoise` stage (`:80-104`) makes almost no difference inside a passband this narrow.

**F4. Callers bypass the receiver filter.** *Open, Phase 3.*
- Only noise goes through the filter; callers are added unfiltered (`mixer.rs:395-408`). Narrowing `noise_bandwidth` changes only the noise, so it can't be used to pull one caller out of a pileup the way a real receiver filter can.

**F5. No AGC.** *Open, Phase 5.*
- Real receiver gain drops under strong signals and static crashes, then recovers according to threshold, attack, hang and release. Pronounced pumping between characters is not a universal realism requirement.
- The mixer sums sources at fixed levels with only a soft clipper at 0.8 (`mixer.rs:440-450`).

**F6. Noise-floor modulation was effectively zero.** *Fixed in Phase 1.*
- `noise.rs:274-277` picked a new random target **every sample** and smoothed it with a 0.3 Hz one-pole low-pass. The targets averaged out: peak `|noise_floor|` was 0.007, so `floor_mod` stayed within 1 ± 0.0008 at QRN 0.3.

**F7. Pops were square pulses, not impulses.** *Fixed in Phase 1.*
- `pop_sample()` (`noise.rs:217-226`) held a flat DC step for 1–5 ms and ramped down only over the last 5 samples. After the bandpass this sounded like two clicks, one at each edge. The doc comment ("sharp attack, quick decay") didn't match.

**F8. Crashes are uniform noise bursts.** *Open, Phase 2.*
- `maybe_start_crash()` / `crash_sample()` (`:169-214`) produce one burst of uniform white noise with exponential decay over 50–200 ms. A clustered impulsive model is a candidate for a more varied sound; its distribution, timing and level need listening comparison.

**F9. Configured filter was not applied at startup.** *Fixed in Phase 1.*
- `NoiseGenerator::new` hard-coded 600 Hz / 400 Hz (`noise.rs:138`), and the filter was only updated when an `AudioCommand::UpdateSettings` arrived. Users with a non-default tone or bandwidth heard the wrong filter until they changed a setting.

## 4. Completed work

### 4.1 Phase 1: noise generator fixes (`9467a9b`)

| Item | Finding | Change | Regression test (`src/audio/noise.rs`) |
|---|---|---|---|
| 1a | F9 | `NoiseGenerator::new(sample_rate, center_hz, bandwidth_hz)`; `Mixer::new` passes `tone_frequency_hz` and `noise_bandwidth`. | `noise_filter_uses_configured_passband_at_startup` |
| 1b | F1, F6 | Sine oscillators and the post-filter white term removed. QRN is now white excitation added to the band noise **before** the filter. Slow level drift is a gain envelope: a held random target (new target every 0.2–2 s) smoothed by the existing 0.3 Hz one-pole, giving up to ±`6 * qrn_intensity` dB. | `all_noise_effects_are_band_limited_at_supported_sample_rates`, `qrn_produces_slow_background_level_variation` |
| 1c | F7 | Pops are random-polarity exponential excitations, time constant 0.2–0.5 ms, tail of 8 time constants, timed in seconds so energy is comparable at 44.1/48 kHz. Filter ringing is expected. | `pops_decay_immediately_with_comparable_energy_across_sample_rates` |
| 1e | — | Test-only `NoiseGenerator::with_seed(...)` for deterministic tests and probes. | `seeded_noise_is_independent_of_buffer_boundaries` |
| — | — | Diagnostic spectrum report (ignored by default; see §9.2). | `report_noise_spectrum` |

No settings fields were added and saved values are retained.

### 4.2 Phase 1 follow-ups

Found in review on 2026-11-02 and fixed in the follow-up commit.

| Item | Change | Regression test |
|---|---|---|
| 20 Hz test margin | Background-only renders measure about −19.3 to −19.7 dB at 20 Hz: that is the 2nd-order filter's own skirt, so the old fixed 20 dB threshold passed only because the test used the heavy-effects render. The check now compares the heavy-effects render against a background-only render through the same filter and fails if effects add more than 3 dB of low-frequency energy. (The old QRN path put 20 Hz about 24 dB *above* that reference.) | `all_noise_effects_are_band_limited_at_supported_sample_rates` |
| 5 kHz threshold | The original proposal said "at least 15 dB" but the code asserted `center / 30` (14.8 dB). Thresholds are now written in dB; the check is a true 15 dB. | same |
| Sample-rate normalization | White excitation (background, QRN, crash bursts) is scaled by `sqrt(fs / 44100)` so noise power per Hz is constant. `PinkNoise` adapts its poles and gains to the device rate, keeping each stage's corner frequency and DC gain. Output at 44.1 kHz is bit-identical to `9467a9b`. | `background_level_is_independent_of_sample_rate`, `crash_level_is_independent_of_sample_rate`, `pink_noise_response_is_independent_of_sample_rate` |

The size of the sample-rate error was smaller than first estimated for the background. The fixed pink coefficients made their 1/f components *louder* at higher rates, roughly cancelling the 1/fs loss of the white term. Measured background error before the fix was +0.06 dB at 48 kHz and +0.35 dB at 96 kHz. Pure-white sources had the full 1/fs dependence: crash-dominated renders were −0.24 dB at 48 kHz and −1.37 dB at 96 kHz. After the fix all of these are within ±0.11 dB. The pink-stage response matches the 44.1 kHz design within 0.1 dB up to 1.5 kHz, the highest passband edge the UI allows (it is about −0.1 dB at 3 kHz at 96 kHz).

Current noise path (`s = sqrt(fs / 44100)`):

```
white * s -> PinkNoise(fs) --+
white * s * 0.3 * qrn -------+--> * floor_gain(qrn) --+
crash (white * s) ------------------------------------+--> 2nd-order bandpass -> * level
pop (exponential, timed in seconds) ------------------+
```

### 4.3 Phase 1 acceptance criteria as implemented

These are the thresholds the code actually asserts, measured at the generator output before mixer dither and clipping, after 1 s warm-up, at 44.1 and 48 kHz unless noted:

- All effects at maximum (crash 5/s, pop 10/s, all intensities 1.0), 600 Hz / 350 Hz: power at 5 kHz at least 15 dB below center. The 2nd-order filter alone gives about 23.4 dB of rejection there; a 30 dB target belongs with the Phase 3 filter.
- Same render: 20 Hz power relative to center no more than 3 dB above a background-only render's, and mean within 0.0001 of zero. This is a low-frequency leakage check, not a direct measurement of the 0.3 Hz drift.
- Background-only RMS at 48 and 96 kHz within 0.2 dB of 44.1 kHz; crash-dominated RMS within 0.75 dB; pink-stage magnitude response within 0.1 dB of the 44.1 kHz design from 100 Hz to 1.5 kHz.
- 1-second RMS coefficient of variation with `qrn_intensity = 1.0` is above 0.1 and more than 3× the value at `qrn_intensity = 0`.
- A pop decays from its first sample (no plateau), and its filtered energy at 44.1 vs 48 kHz is within a 0.9–1.1 ratio.
- A mixer constructed at 1 kHz / 100 Hz has more than 10× the noise power at 1 kHz than at 600 Hz, before any settings update.
- Seeded output is identical whether rendered in one buffer or in 137-sample chunks.

### 4.4 Known characteristics of the current state

These are expected for an intermediate state and are addressed by later phases.

1. **QRN is nearly inert at its default.** At `qrn_intensity = 0.3`, QRN adds about +0.3 dB of RMS (0.006301 → 0.006718 at 44.1 kHz) plus up to ±1.8 dB of slow drift. Inside the passband, the QRN white term sounds the same as raising `noise_level`. Its distinctive sound waits for the Phase 2 crackle stream.
2. **Overall loudness dropped.** Total generator RMS at defaults fell from about 0.028 to about 0.007, but most of the removed energy was sub-audio or above 2 kHz. Passband noise is roughly unchanged, so caller SNR (F2) is essentially as before. The background sounds quieter and less hissy; final levels are set in Phase 4.

## 5. Remaining work (in execution order)

### Phase 2: QRN character (F1 remainder, F8)

**2a. Crackle stream.** A Poisson stream of small impulses injected before the filter. Rate scales with `qrn_intensity` (e.g. 0–150 per second). Amplitudes come from a heavy-tailed distribution (e.g. Pareto or log-normal), capped to avoid runaway peaks. Through the filter these become atmospheric crackle. Reuse the Phase 1 pop excitation shape so energy stays consistent across sample rates. Decide whether the plain QRN white term stays once crackle exists.

**2b. Crashes as impulse clusters.** A crash event lasting 100–600 ms with an envelope (attack under 5 ms, exponential decay). Inside it, a dense Poisson stream of heavy-tailed impulses (e.g. 500–3000 per second) scaled by the envelope and `crash_intensity`.

**Acceptance criteria:**
- The Phase 1 band-limit, mean, partition and sample-rate tests still pass.
- With `qrn_intensity = 1.0`, the kurtosis (peakiness) of filtered output is clearly above that of Gaussian noise, and above the value at `qrn_intensity = 0`.
- Crackle and crash energy per event is comparable at 44.1/48 kHz.
- Impulse amplitudes are capped; worst-case peak at maximum settings is stated.
- Listening review per §6.

### Phase 3: Shared receiver filter (F3, F4)

**3a. Filter design.** Move the filter out of `NoiseGenerator` into `Mixer`, and apply it to **noise + callers**, not to the user sidetone.
- First specify passband loss and ripple, stopband rejection, and transition width. Then choose a true bandpass design implemented as cascaded second-order sections, and validate its response numerically.
- Independently cascading a 4th-order high-pass at 425 Hz and a 4th-order low-pass at 775 Hz gives only about 7.1 dB of rejection at 950 Hz relative to 600 Hz. It is not equivalent to a Butterworth bandpass transformation.
- **Order estimate (analytical; confirm numerically when designing):** for 600 Hz / 350 Hz, an 8th-order Butterworth bandpass (4th-order prototype) gives roughly 19 dB of rejection at +BW (950 Hz) but roughly 39 dB at −BW (250 Hz), because the bandpass response is asymmetric on a linear frequency axis. Meeting 20 dB at +BW likely needs a 10th-order Butterworth (≈24 dB) or a Chebyshev/elliptic design (decision D6).
- Place the band edges so their **geometric** center, √(low × high), equals the tone frequency. Edges at center ± BW/2 put the geometric center at about 574 Hz for 600 Hz / 350 Hz.
- Define valid cutoffs: `0 < low < high < sample_rate/2`. Current controls allow a 400 Hz center with 1,000 Hz bandwidth, so `f0 - BW/2` would be negative. Constrain the controls or define an explicit mapping.
- Normalize in-band gain. Define how a settings change preserves filter state or crossfades without clicks.
- Remove the generator's filter when the shared filter takes over; don't filter noise twice.
- Watch for `f32` instability at narrow bandwidths. Consider computing coefficients in `f64`, or running filter state in `f64`.
- Optional "filter shape" setting (soft / normal / sharp) to choose the order.

**3b. Mixer restructure.** Render RX (noise + callers) into a reusable scratch buffer, filter it, then add sidetone, master volume, dither and clipper (AGC slots in after the filter in Phase 5). Allocate scratch storage outside the audio callback, or reuse existing capacity.
- Define the stage where RX muting is applied, and whether filter, noise and (later) AGC clocks advance while muted. Today noise is not generated while muted, so its state freezes, while callers keep advancing.
- Preserve caller and user segment completion events independently of audible muting.

**Acceptance criteria:**
- A caller offset by ±BW from center is attenuated by at least 20 dB (or the threshold chosen in D6) relative to an on-frequency caller, for valid positive audio frequencies. Passband ripple/loss is within the chosen spec. Measure before dither and clipping.
- Callers at the default spread edges are measured and reported. Offsets are drawn from `-frequency_spread_hz/2 .. frequency_spread_hz/2`, so the default 300 Hz spread means ±150 Hz, not ±300 Hz. Measure this loss before adjusting difficulty defaults.
- Explicit tests for sidetone level and timing, segment completion order, station completion during muted RX, TX/RX transitions, buffer partitioning, and filter changes. The baseline has Morse tests but no segment-completion tests to rely on.

### Phase 4: Calibrate signal-to-noise ratio (F2)

Depends on Phase 3 (the reference filter). Generator noise is already normalized to constant power per Hz across sample rates (§4.2); the Phase 3 filter and any new excitation (Phase 2 crackle) must preserve that.

**4a. Noise reference.** Define a fixed stationary-noise reference RMS in a specified receiver response (proposed: the Phase 3 filter at 500 Hz bandwidth). State its equivalent noise bandwidth rather than assuming the nominal bandwidth determines integrated noise. Measure caller reference strength during on-frequency key-down, before QSB, AGC, dither and clipping, with impulsive effects and drift disabled. Narrowing the actual receiver filter should reduce integrated noise without rescaling the caller.

**4b. Caller strength as SNR.** Add `snr_min_db` and `snr_max_db` to `SimulationSettings` with serde defaults. With reference noise RMS σ, sine peak amplitude is `sqrt(2) * σ * 10^(snr_db/20)`.
- Keep this amplitude independent of the live noise slider and the current bandwidth. Raising noise should lower actual SNR, and muting noise must not mute callers.
- Preserve each caller's selected strength across repeats.
- Legacy conversion needs explicit version or field-presence handling; see §7.

**4c. UI.** The noise slider covers a meaningful range (e.g. −20 to +10 dB around the reference), labeled in dB. Caller strength sliders are labeled as SNR in dB.

**4d. Headroom.** Check the five strongest simultaneous callers plus noise and capped impulsive events. Define a peak ceiling and allowed clipping before finalizing levels. AGC attack is finite and does not by itself prevent initial peak overload.

**Acceptance criteria:**
- With the reference filter and noise level, on-frequency key-down SNR measures within ±1 dB of the configured value before AGC, at 44.1, 48 and 96 kHz.
- At other bandwidths and noise settings, test the expected change in actual SNR rather than forcing it back to the configured value. Test noise-off behavior separately.
- At default settings, the weakest callers are clearly near the noise level when listening (§6).

### Phase 5: AGC (F5)

After the filter, specify detector type (peak or RMS), threshold, gain limits, initial gain and smoothing. Candidate timing: 2–5 ms attack, 150–400 ms release, with explicit hang behavior; validate by listening.
- Define behavior below threshold so ambient noise is not continuously boosted to the target. Define recovery after long TX or muted-RX periods. A bare `target / max(env, floor)` expression doesn't specify these.
- Setting `agc: { enabled, release_ms }` with serde defaults.
- Strong callers and crashes may reduce gain, followed by controlled recovery. Avoid audible gain steps, excessive noise breathing, and unstable gain during silence.

**Acceptance criteria:**
- Measured gain reduction for a strong caller and bounded recovery after it ends, against the chosen timing contract. Attack overshoot is tested separately from the release envelope.
- Pronounced recovery between every character is not required.

### Phase 6 (optional): Background QRM

The noise on a busy contest band is largely other signals: distant CQs, partial exchanges, stations near the filter edge.
- A QRM source of non-interactive CW stations started at random times. Each sends short CQ or exchange fragments at random offsets (inside and outside the passband) at low SNR (e.g. −5 to +10 dB), through the Phase 3 filter.
- Implement as `ActiveStation` instances flagged so their completion is not reported in `completed_stations`, leaving caller and QSO logic unchanged.
- Settings: `qrm: { enabled, density, snr_min_db, snr_max_db }`.

## 6. Validation plan

1. **Automated:** seeded generators; record all parameters; discard a specified warm-up; report the measured stage and the spectral normalization. Exercise 44.1/48 kHz (and 96 kHz for level and response checks), buffer boundaries, filter extremes, and mute/settings transitions. The §9.2 probe is diagnostic, not a listening-quality assertion.
2. **Listening review:** save ~30 s WAV clips (debug-only, or a test that writes into `target/`) at defaults and at "heavy QRN" for each phase. Have experienced CW contesters compare them, ideally next to real off-air recordings of a contest band through a 400–500 Hz filter (decision D10). Checklist:
   - Is the unfiltered high-frequency hiss gone without new audible transients? Some stopband energy remains. Mixer dither is uniform (not triangular as its code comment says), about ±0.0005, added after the filter, so it is flat broadband hiss. At low `noise_level` it dominates above ~2 kHz. Don't require mathematical silence above 2 kHz.
   - Does narrowing the bandwidth make the noise sound narrower and more hollow, with ringing?
   - Are weak callers genuinely hard to copy at the low end of the default SNR range?
   - Do crashes sound like lightning? Does the AGC pump and recover believably?
3. **Performance:** an 8th–10th-order filter plus AGC at 48 kHz is cheap, but confirm there are no audio dropouts on the slowest supported machine.

## 7. Compatibility and migration

Because everything ships in one release, migration runs once, from the v0.7.0 settings format straight to the final format. Intermediate phases don't need their own migration or release notes.

Settings whose meaning changes across the effort:

| Setting | Changed by | Change |
|---|---|---|
| `qrn_intensity` | Phase 1 (done), Phase 2 | Filtered excitation plus level drift; then crackle |
| `crash_intensity` | Phase 2 | Clustered impulses instead of one noise burst |
| `noise_bandwidth` | Phase 3 | Now filters callers too, possibly with a different filter shape |
| `noise_level` | Phase 4 | Becomes a dB value relative to the reference |
| `amplitude_min` / `amplitude_max` | Phase 4 | Replaced by `snr_min_db` / `snr_max_db` |
| new: `agc`, filter shape, (`qrm`) | Phases 3, 5, (6) | Added with serde defaults |

- Old settings files must still load. Serde defaults alone can't decide whether legacy values need conversion, so use a single config version bump (or field-presence detection) and apply the D4/D8 policy once. Specify precedence if both amplitude and SNR settings exist, never overwrite an intentional new value, and keep the original file recoverable.
- One set of release notes describes the final result: the changed noise sound, filtered callers, SNR-based strength, AGC, and how to restore an "easy" setup. Difficulty will change noticeably.

## 8. Decisions needed

Grouped by what they block. A proposed default is given for each open decision; "defaults fine" is a valid answer.

**Resolved**
- **D1. Release scope.** *Decided 2026-11-02:* all phases stay on `improve-noise-realism` and ship together as one release; nothing merges until complete.
- **D7. AGC packaging.** *Resolved by D1:* AGC is its own milestone (one or more commits) on the same branch, after the receiver filter.

**Blocks Phase 3**
- **D5. Filtering callers.** Always on, or behind a setting at first since it changes difficulty? *Proposed: behind a setting, default on.*
- **D6. Filter spec.** Keep "≥20 dB at ±BW" (likely 10th-order Butterworth, or Chebyshev), or loosen it? *Proposed: keep 20 dB; choose order numerically.*

**Blocks Phase 4**
- **D2. Reference bandwidth.** Define SNR in a 500 Hz receiver response, and turn `noise_level` into a dB slider? *Proposed: yes to both.*
- **D3. Default SNR range.** 10–35 dB (easier) or 3–30 dB? Note that 3 dB in 500 Hz is still fairly easy copy for experienced operators. *Proposed: 10–35 dB default, with "contest" (3–30 dB) and "expert" (about 0–20 dB) presets.*
- **D4. Legacy `amplitude_min`/`amplitude_max`.** Convert to SNR automatically once, or deprecate them and switch to new defaults? *Proposed: deprecate and switch to new defaults, with a release note.*
- **D8. Settings migration for the noise section.** Reset once for existing users, or keep their values under the new meaning? *Proposed: one-time reset of the noise section via a config version bump.*

**Cross-cutting**
- **D9. QRM (Phase 6).** Part of this release, or a separate effort after it merges? Including it delays the release; leaving it out means the "realistic band" release still has no off-frequency signals. *Proposed: separate effort after this branch merges, unless listening reviews say the band sounds empty without it.*
- **D10. Reference recordings.** Do we have, or can we get, licensed off-air contest recordings as a listening benchmark?

## 9. Appendix: measurements

### 9.1 Running the probe

```sh
cargo test --locked audio::noise::tests::report_noise_spectrum -- --ignored --nocapture
```

Four cases: background only, background plus QRN at 0.3, full defaults, and heavy effects (crash rate 5/s, pop rate 10/s, all intensities 1.0). Every case uses seed 42, 600 Hz center, 350 Hz bandwidth, output level 0.25, at 44.1 and 48 kHz. Each render discards 1 s of warm-up, then measures 20 s of generator output, before mixer dither, clipping, sidetone or callers.

Total RMS and mean are time-domain measurements. Spectral values average Hann-windowed 4096-sample DFT power over all complete frames, in dB relative to the same render's 600 Hz power. These are output spectra, not dBFS, isolated component powers, or filter transfer functions. Total RMS is not audible-band RMS. Resolving sub-audio oscillations would need a separate, longer-window analysis.

### 9.2 Results after the Phase 1 follow-ups

44.1 kHz output is bit-identical to `9467a9b`. 48 kHz values changed with sample-rate normalization; the `9467a9b` 48 kHz RMS values are kept in the third column for comparison.

Total RMS (mean is below 1e-6 in every case):

| Case | 44.1 kHz | 48 kHz | 48 kHz at `9467a9b` |
|---|---:|---:|---:|
| Background only | 0.006301 | 0.006303 | 0.006344 |
| Background + QRN 0.3 | 0.006718 | 0.006675 | 0.006688 |
| Defaults | 0.006883 | 0.007005 | 0.007018 |
| Heavy effects | 0.012280 | 0.011921 | 0.011671 |

Spectrum in dB relative to 600 Hz, 44.1 kHz / 48 kHz:

| Freq (Hz) | Background only | Background + QRN 0.3 | Defaults | Heavy effects |
|---:|---:|---:|---:|---:|
| 20 | -19.34 / -19.67 | -19.99 / -19.35 | -19.63 / -20.01 | -23.29 / -23.70 |
| 100 | -12.19 / -12.76 | -13.04 / -12.54 | -12.50 / -13.07 | -15.68 / -15.66 |
| 300 | -5.14 / -5.38 | -5.52 / -5.38 | -4.85 / -6.27 | -6.87 / -6.72 |
| 1000 | -8.78 / -9.67 | -9.02 / -8.28 | -7.63 / -8.58 | -7.13 / -7.90 |
| 5000 | -32.06 / -32.21 | -30.61 / -30.18 | -30.53 / -30.31 | -25.98 / -25.87 |
| 12000 | -46.03 / -46.31 | -42.73 / -41.57 | -42.13 / -42.41 | -35.93 / -35.98 |
| 18000 | -57.42 / -55.80 | -51.81 / -49.86 | -51.41 / -50.41 | -45.57 / -43.43 |

Run-to-run differences between the two rates (different random sequences) are typically 0.3–1 dB per bin in these single-bin estimates; they are not response differences.

Compared with §3.2: at full defaults, 5 kHz went from −7.1 dB (measured relative to the 600 Hz bin's +4.7 dB, so about −11.8 dB relative) to about −30.5 dB relative. 12 and 18 kHz improved by a similar or larger margin. The unfiltered hiss path is gone. The 2nd-order filter's skirt now dominates the 20–100 Hz values (see §4.2).
