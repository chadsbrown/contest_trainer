# Noise Realism: Listening Test Plan

- **Purpose:** decide by ear whether the Phase 2 noise (crackle and crash clusters) sounds realistic, and collect the decisions needed to start Phase 3.
- **Branch:** `improve-noise-realism`, tested at `eef5c63` or later. All work stays on this branch and ships as one release.
- **Companion doc:** [`NOISE_REALISM_PROPOSAL.md`](NOISE_REALISM_PROPOSAL.md) has the full plan, findings and measurements. Section references (§) below point there.
- **Written:** 2026-11-02, at the end of a working session, so testing and the next session can pick up without the conversation that produced it.

## 1. Where things stand

| Done | Commit | What it changed |
|---|---|---|
| Phase 1 | `9467a9b` | QRN filtered (no more unfiltered hiss or sub-audio wobble); configured filter applied at startup; pops are real impulses; slow level drift works |
| Phase 1 follow-ups | `089fea8` | Noise level independent of sample rate; sturdier tests |
| Listening clips | `4fe0f6e` | `cargo test` renders WAV clips through the real mixer |
| Phase 2a | `cf5a0cf` | QRN is now crackle (sparse static ticks) plus slow fading |
| Phase 2b | `5fa58ab` | Crashes are bursts of impulses with a sharp onset and decaying tail |

**Not done yet**, so expect these in any test:

| Behavior you will hear | Why | Fixed by |
|---|---|---|
| Callers are far louder than the noise (about 30+ dB SNR) | Caller strength isn't calibrated against noise | Phase 4 |
| Narrowing the bandwidth affects only the noise; off-frequency callers stay loud | Callers bypass the receiver filter | Phase 3 |
| The filter sounds soft, with gentle skirts | Single 2nd-order bandpass | Phase 3 |
| Distortion when several callers overlap; harsh clipping on big crashes at high settings | Output only has a soft clipper (knee 0.8) | Phase 4 headroom, Phase 5 AGC |
| Crashes don't make the receiver "duck" | No AGC | Phase 5 |
| No distant signals or QRM | Not modeled | Phase 6 (optional) |
| Settings tooltip calls QRN "Atmospheric noise rumble" | UI text predates the change | Phase 4 UI rework |

Judge the **noise itself**: its texture, crackle, crashes and fading. Don't judge caller levels, filter shape or clipping yet.

## 2. Setup

### 2.1 Render the clips

```sh
git checkout improve-noise-realism && git pull
cargo test --locked listening_clips -- --ignored --nocapture
```

This writes 48 kHz, 16-bit mono WAVs and a `clips.txt` manifest (revision, every setting, measured peak/RMS) to `target/listening-clips/`. Noise is seeded, so renders are repeatable.

| Clip | Contents | Listen for |
|---|---|---|
| `01-noise-defaults` | Noise only, default settings | Overall realism at defaults |
| `02-noise-background-only` | Noise only, all effects off | Reference: plain band hiss |
| `03-noise-max-effects` | Noise only, every effect slider at maximum | Heavy static: crackle density, crash character |
| `04-pileup-defaults` | Scripted pileup, defaults | Noise in context |
| `05-pileup-max-effects` | Pileup, every effect at maximum | Static against signals |
| `06-pileup-max-noise-level` | Pileup, noise level 0.5 | Highest noise reachable today |
| `07-pileup-narrow-200hz` | Pileup, 200 Hz bandwidth | Narrow-filter noise sound (ignore that callers aren't filtered) |

### 2.2 Playback

- Use headphones or decent speakers. Laptop speakers can't reproduce the 400–800 Hz band and ticks faithfully.
- **Pass A (as the app plays it):** set a comfortable volume on `04-pileup-defaults`, then play every clip without touching the volume. Noise-only clips will be quiet (about −46 dBFS RMS); that is how they sound in the app today.
- **Pass B (texture):** raise the volume about 20 dB for the noise-only clips (01–03) to judge the character of the crackle and crashes. Protect your ears before switching back to a pileup clip.
- Optional: a spectrogram helps tie what you hear to events:
  `ffmpeg -i target/listening-clips/03-noise-max-effects.wav -lavfi showspectrumpic=s=1200x400:stop=3000 /tmp/spec.png`

### 2.3 Live in the app (optional but useful)

The clips cover fixed scenarios; the app lets you move sliders live.

1. **Back up your settings first.** The app saves settings on change, and `master` and this branch share the same file: `~/.config/contest_trainer/settings.toml` on Linux (`dirs::config_dir()` elsewhere).
   `cp ~/.config/contest_trainer/settings.toml ~/.config/contest_trainer/settings.toml.bak`
2. `cargo run --release`, then File → Settings → Static/QRN Settings. F1 or Enter sends a CQ to bring in callers.
3. Sweep each slider on its own from 0 to max while listening: QRN Intensity, Crash Rate/Intensity, Pop Rate/Intensity, Noise Level, Noise Bandwidth.
4. Restore the backup afterwards if you want your old settings.

### 2.4 Before/after comparison with `master` (optional)

`master` has the old noise but no clip tool. To hear it, run the app from a separate worktree with the **same settings file**:

```sh
git worktree add ../contest_trainer-master master
cd ../contest_trainer-master && cargo run --release
# afterwards: git worktree remove ../contest_trainer-master
```

The old QRN was mostly unfiltered broadband hiss with slow swelling (§3.3 F1). The new version should sound narrower, with no hiss above about 2 kHz.

### 2.5 Real-world reference (decision D10)

The best benchmark is real off-air audio of a CW contest band through a 400–500 Hz filter: recorded from your own receiver, a web SDR, or licensed recordings. Note the filter bandwidth and band conditions of whatever you use. If none is available, rely on operating experience and say so in the results.

## 3. What to judge

For each item, note whether it sounds right, too much or too little, and anything that sounds synthetic. The "knob" column is what gets changed if it's wrong; you don't need to edit code yourself.

| # | Question | Clips | Knob (in `src/audio/noise.rs`) | Current value |
|---|---|---|---|---|
| Q1 | Background hiss: does it sound like a quiet band through a CW filter? Any remaining hiss above ~2 kHz? | 02, 01 | Pink stage; filter (Phase 3) | — |
| Q2 | Crackle density: right number of ticks at defaults (QRN 0.3, about 45/s) and at max (150/s)? Ticks or "frying"? | 01, 03 | `CRACKLE_MAX_RATE` | 150 /s at QRN 1.0 |
| Q3 | Crackle loudness relative to the hiss | 01, 03 | `CRACKLE_LEVEL_DB` | 0 dB vs background at QRN 1.0 |
| Q4 | Largest ticks: occasional loud ticks (about one per 20 s at defaults) — realistic, too startling, or too tame? | 01 | `IMPULSE_PARETO_ALPHA` (lower = heavier tail), `IMPULSE_MAGNITUDE_CAP` | α 3.0, cap 10× |
| Q5 | Crash loudness at defaults (intensity 0.2) and at max | 01, 03, 05 | `CRASH_LEVEL_DB` | Loudest crash +20 dB vs background at intensity 1.0 |
| Q6 | Crash texture: like lightning (rough, crackly onset; smooth decaying roar)? Too grainy or too smooth? | 03 | `CRASH_IMPULSE_RATE` (lower = grainier) | 500–3000 /s per crash |
| Q7 | Crash length and shape: too long or short? Onset sharp enough? Tail natural? | 03 | `CRASH_DURATION`, `CRASH_ATTACK`, `CRASH_END_LEVEL` | 0.1–0.6 s, 1–5 ms, ends at −26 dB |
| Q8 | Pops: distinct from crackle, or redundant now? | 01, 03 | Pop settings; pop shape in `maybe_start_pop` | τ 0.2–0.5 ms |
| Q9 | Slow fading: noticeable? Too much or too little at QRN 1.0? | 03 (listen over 30 s) | Drift depth in `noise_floor_gain` (±`6 · qrn` dB), hold 0.2–2 s | ±6 dB at QRN 1.0 |
| Q10 | Narrow filter: at 200 Hz, does the noise sound narrower and more "hollow"? | 07 vs 04 | Filter (Phase 3) | 2nd-order bandpass |
| Q11 | Overall: compared with real bands (or `master`), is this more realistic? What is most wrong? | all | — | — |

Slider defaults for reference: noise level 0.25, bandwidth 350 Hz, tone 600 Hz, crash rate 0.4/s, crash intensity 0.2, pop rate 0.6/s, pop intensity 0.73, QRN intensity 0.3.

## 4. Decisions to bring back

Answer these with the results. Each has a proposed default in proposal §8; "defaults fine" is a valid answer.

| ID | Question | Needed for | Proposed |
|---|---|---|---|
| D11 | Phase 2 tuning: any changes from §3 above? | Closing Phase 2 | Keep current values unless they sound wrong |
| D5 | Should callers go through the receiver filter always, or behind a setting at first? | Phase 3 | Behind a setting, on by default |
| D6 | Keep "≥ 20 dB rejection one bandwidth from center" as the filter target? (Probably needs a 10th-order filter.) | Phase 3 | Keep 20 dB |
| D2 | Define SNR in a 500 Hz bandwidth, and make the noise slider a dB value? | Phase 4 | Yes to both |
| D3 | Default caller SNR range | Phase 4 | 10–35 dB, plus "contest" (3–30) and "expert" (~0–20) presets |
| D4 | Old `amplitude_min`/`amplitude_max` settings: convert or drop? | Phase 4 | Drop, use new defaults, release note |
| D8 | Reset users' saved noise settings once at release? | Phase 4 | Yes, via a config version bump |
| D9 | Background QRM in this release, or after? | Phase 6 | After, unless the band sounds empty without it |
| D10 | Reference recordings available? | Validation | — |

Only D11, D5 and D6 are needed to continue right away. The rest can wait until Phase 4.

## 5. Recording results

Fill this in (or write the same information anywhere convenient) and commit it on the branch. The next session starts from here.

**Tested at commit:** ______  **Date:** ______
**Playback:** (headphones/speakers) ______
**Reference used:** (off-air / web SDR / `master` / memory only) ______

| # | Verdict (OK / too much / too little / wrong) | Notes |
|---|---|---|
| Q1 | | |
| Q2 | | |
| Q3 | | |
| Q4 | | |
| Q5 | | |
| Q6 | | |
| Q7 | | |
| Q8 | | |
| Q9 | | |
| Q10 | | |
| Q11 | | |

| Decision | Answer |
|---|---|
| D11 | |
| D5 | |
| D6 | |
| D2, D3, D4, D8, D9, D10 (optional now) | |

Other observations:

## 6. Resuming work

In a new session on this branch, a prompt like the following carries the full context:

> Read `docs/NOISE_LISTENING_TEST_PLAN.md` (results in §5) and `docs/NOISE_REALISM_PROPOSAL.md`. Apply any Phase 2 tuning from the results, re-render the clips, then start Phase 3.

What the next session will do with the results:
1. Change the named constants for any Q2–Q9 item marked wrong, re-render the clips, and record the new values in proposal §4.6. Re-run `cargo test --locked`. The calibration and sample-rate tests don't depend on the tuning values. `worst_case_peak_is_bounded` (bound 8) may need revisiting if crash loudness or the tail goes up.
2. Mark D11 resolved in proposal §8 and record the answers to D5/D6 (and any others).
3. Start Phase 3 (shared receiver filter, proposal §5) using the D5/D6 answers.

Working conventions so far: commit and push to `improve-noise-realism` as each step completes; nothing merges to `master` until all phases are done.
