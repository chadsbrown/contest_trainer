# CW Contest Trainer -- Application Specification

This document defines the complete specification for the CW Contest Trainer application. It is intended to be sufficient for recreating the application from scratch using any implementation language or framework, given only these specifications.

---

## 1. Overview

**Name:** CW Contest Trainer  
**Purpose:** A cross-platform Morse code (CW) contest simulator for amateur radio operators. Users practice realistic contest operating by calling CQ, copying callsigns and exchanges from simulated callers, and logging QSOs under realistic audio conditions.

**Window Title:** "CW Contest Trainer"  
**Initial Window Size:** 640 x 375 pixels  
**Minimum Window Size:** 400 x 280 pixels

---

## 2. Technology Stack

| Component | Technology |
|-----------|-----------|
| Language | Rust (Edition 2021) |
| GUI Framework | egui 0.33 + eframe 0.33 (immediate-mode GUI) |
| Audio | cpal 0.15 (cross-platform audio) |
| Threading IPC | crossbeam-channel 0.5 (bounded channels, capacity 64) |
| Serialization | serde 1.0 + toml 0.8 |
| Date/Time | chrono 0.4 |
| Random | rand 0.8 (with `small_rng` feature) |
| File Dialog | egui-file-dialog 0.12 |
| System Dirs | dirs 5 |

### Build Configuration

- On Windows release builds, the console window is hidden (`windows_subsystem = "windows"`).
- A build script auto-discovers contest modules at compile time (see Section 12).

### Platform Dependencies (Linux)

```
libasound2-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev
```

---

## 3. Architecture

### 3.1 Module Structure

```
src/
  main.rs              -- Entry point, window creation
  app.rs               -- Main application struct, event loop, keyboard handling
  config.rs            -- Settings structures with serialization
  state.rs             -- State machine enums and QSO context
  stats.rs             -- Session statistics collection and analysis
  messages.rs          -- IPC message types between UI and audio threads
  export.rs            -- Session export to markdown
  cty.rs               -- Country/DXCC entity lookup from cty.dat
  audio/
    engine.rs          -- Audio thread, cpal integration
    mixer.rs           -- Multi-station audio mixing
    morse.rs           -- Morse code encoding and tone generation
    noise.rs           -- Background noise, static, QRN generation
  contest/
    types.rs           -- Contest trait and shared types
    callsign.rs        -- Generic callsign pool and file parsing
    cwt.rs             -- CWT contest implementation
    cqww.rs            -- CQ World Wide implementation
    cqwpx.rs           -- CQ WPX implementation
    sweepstakes.rs     -- ARRL Sweepstakes implementation
    arrldx.rs          -- ARRL DX CW implementation
  station/
    caller_manager.rs  -- Caller queue, patience, retry logic
  ui/
    main_panel.rs      -- Main QSO panel rendering
    settings_panel.rs  -- Settings window rendering
    stats_window.rs    -- Statistics window rendering
    export_dialog.rs   -- Export result modal dialog
```

### 3.2 Threading Model

The application uses two threads connected by bounded channels (capacity 64 each):

1. **UI Thread** -- Runs the GUI event loop, handles keyboard input, manages state machine, sends audio commands.
2. **Audio Thread** -- Synthesizes CW tones, mixes multiple stations, generates noise, reports completion events.

**Command channel** (UI -> Audio): `AudioCommand`  
**Event channel** (Audio -> UI): `AudioEvent`

### 3.3 Information-Driven State Machine

The state machine uses approximately 9 states that describe *who is transmitting/waiting*, combined with rich context objects that track progress and situation details. This is an information-driven design (not action-driven), keeping the state count minimal while supporting flexible user actions.

---

## 4. State Machine

### 4.1 States

```
ContestState (default: Idle)
  Idle                                -- Waiting for user to start
  CallingCq                           -- User sending CQ message
  WaitingForCallers                   -- CQ finished, waiting for stations
  StationsCalling                     -- Station(s) calling us
  UserTransmitting { tx_type }        -- User is transmitting
  WaitingForStation                   -- Brief pause before station responds
  StationTransmitting { tx_type }     -- Station is transmitting
  QsoComplete                         -- QSO complete, TU sent
  WaitingForTailEnder                 -- Pause before tail-ender starts
```

### 4.2 User Transmission Types

```
UserTxType
  Exchange       -- Their call + our exchange (Enter key)
  CallsignOnly   -- Just their callsign (F5)
  ExchangeOnly   -- Just our exchange (F2)
  Agn            -- AGN/? request (F8)
  Tu             -- TU message (F3)
```

### 4.3 Station Transmission Types

```
StationTxType
  SendingExchange  -- Station sending their exchange
  RequestingAgn    -- Station sending "AGN" or "?"
  Correction       -- Station correcting user's callsign copy
```

### 4.4 QSO Progress Tracking

```
QsoProgress
  sent_their_call: bool           -- We finished sending caller's callsign
  sent_our_exchange: bool         -- We finished sending our exchange
  received_their_call: bool       -- User entered a callsign
  received_their_exchange: bool   -- User entered an exchange
```

Updated by:
- `sent_their_call` / `sent_our_exchange`: Set by audio segment completion events.
- `received_their_call`: Set when user submits callsign (Enter in callsign field).
- `received_their_exchange`: Set when user submits exchange (Enter in exchange field).

### 4.5 QSO Context

```
QsoContext
  progress: QsoProgress
  current_caller: Option<ActiveCaller>     -- The caller we're working
  active_callers: Vec<ActiveCaller>        -- All callers in the pileup
  correction_in_progress: bool             -- Station is correcting our copy
  correction_attempts: u8                  -- How many correction attempts
  wait_until: Option<Instant>              -- Timer for response delays
  expecting_callsign_repeat: bool          -- Caller should repeat their call
  allow_callsign_repeat_ack: bool          -- Caller may send "R R" instead
  caller_exchange_sent_once: bool          -- Caller has sent exchange at least once
  awaiting_user_exchange: bool             -- Waiting for user to send exchange
```

### 4.6 State Transitions

#### Happy Path

```
Idle -[F1/Enter empty]-> CallingCq
CallingCq -[user msg complete]-> WaitingForCallers
WaitingForCallers -[300ms + callers spawn]-> StationsCalling
StationsCalling -[Enter with callsign]-> UserTransmitting{Exchange}
UserTransmitting{Exchange} -[msg complete]-> WaitingForStation (250ms wait)
WaitingForStation -[wait elapsed, SendExchange]-> StationTransmitting{SendingExchange}
StationTransmitting{SendingExchange} -[Enter in exchange field]-> QsoComplete
QsoComplete -[TU msg complete]-> Idle or WaitingForTailEnder
WaitingForTailEnder -[100ms]-> StationsCalling
```

#### All Transitions

| From | Trigger | To |
|------|---------|-----|
| Idle | F1 or Enter (empty) | CallingCq |
| CallingCq | User message complete | WaitingForCallers |
| WaitingForCallers | 300ms elapsed + callers spawn | StationsCalling |
| StationsCalling | Enter (with callsign) | UserTransmitting{Exchange} |
| StationsCalling | F5 | UserTransmitting{CallsignOnly} |
| StationsCalling | F2 | UserTransmitting{ExchangeOnly} |
| StationsCalling | F8 | UserTransmitting{Agn} |
| UserTransmitting{Exchange/CallsignOnly/ExchangeOnly/Agn} | Message complete | WaitingForStation (250ms) |
| UserTransmitting{Tu} | Message complete | Try tail-ender -> Idle or WaitingForTailEnder |
| WaitingForStation | Wait elapsed, CallerResponse::Confused | StationsCalling |
| WaitingForStation | Wait elapsed, CallerResponse::RequestAgn | StationTransmitting{RequestingAgn} |
| WaitingForStation | Wait elapsed, CallerResponse::SendExchange | StationTransmitting{SendingExchange} |
| WaitingForStation | Wait elapsed, correction_in_progress | StationTransmitting{Correction} |
| WaitingForStation | Wait elapsed, expecting_callsign_repeat | StationsCalling |
| WaitingForStation | Wait elapsed, CallerResponse::Wait | StationsCalling |
| StationTransmitting{SendingExchange} | Enter (exchange field) | QsoComplete |
| StationTransmitting{RequestingAgn} | Station complete | StationsCalling |
| StationTransmitting{Correction} | Station complete | StationsCalling |
| QsoComplete | User message complete | Try tail-ender -> Idle or WaitingForTailEnder |
| WaitingForTailEnder | 100ms elapsed | StationsCalling |
| Any state | F1 | CallingCq (resets everything) |
| Any state | Escape | Stop audio only (state unchanged) |

### 4.7 Caller Response Logic

Determines how a caller responds based on QSO progress:

| sent_their_call | sent_our_exchange | Response |
|-----------------|-------------------|----------|
| false | false | Confused (50% sends callsign, 50% sends "?") |
| true | false | RequestAgn (50% sends "AGN", 50% sends "?") |
| false | true | Confused |
| true | true | SendExchange |

**Context override:** If `awaiting_user_exchange == true` AND `sent_their_call == true` AND `sent_our_exchange == false`, returns `Wait` (caller stays silent).

**Special response behaviors in handle_station_response:**

- **Expecting callsign repeat**: 50% chance of "R R" (only if `allow_callsign_repeat_ack`), otherwise sends callsign.
- **Correction in progress**: 75% sends callsign once, 25% sends it twice (`"{call} {call}"`).
- **Random AGN request**: On `SendExchange`, if `caller_exchange_sent_once == false`, probability `agn_request_probability` (default 10%) that caller sends AGN instead of exchange.

### 4.8 Call Correction Flow

When user submits an incorrect callsign:

1. **Correction decision**: Roll against `correction_probability` (default 0.8 / 80%).
2. If correcting AND `correction_attempts < max_correction_attempts` (default 2): set `correction_in_progress = true`, increment attempts.
3. Station sends their callsign: 75% once, 25% twice.
4. Status shows "Fix callsign and press Enter".
5. If user re-enters correctly, correction ends.
6. If user re-enters incorrectly and attempts remain, correction repeats.
7. If attempts exhausted, correction ends (busted call proceeds).

### 4.9 Callsign Similarity Algorithm

Used to match user input to the most likely caller in the pileup:

1. **LCS-based matching**: Greedy forward scan counting matched characters.
2. **Substring bonus**: If either string contains the other, return `shorter_len / longer_len`.
3. **Default formula**: `(2.0 * matches) / (a.len() + b.len())`
4. **Threshold**: `SIMILARITY_THRESHOLD = 0.4` -- callers below this are not considered matches.

### 4.10 Status Display Text and Colors

| State | Text | Color |
|-------|------|-------|
| Idle | "Press F1/Enter to call CQ" | Gray |
| CallingCq | "Calling CQ..." | Yellow |
| WaitingForCallers | "Waiting for callers..." | LightBlue |
| StationsCalling (correction) | "Fix callsign and press Enter" | Orange |
| StationsCalling (normal) | "Station calling - enter callsign" | Green |
| UserTransmitting{Exchange} | "Sending exchange..." | Yellow |
| UserTransmitting{CallsignOnly} (multi) | "Querying partial..." | Yellow |
| UserTransmitting{CallsignOnly} (single) | "Sending callsign..." | Yellow |
| UserTransmitting{ExchangeOnly} | "Sending exchange..." | Yellow |
| UserTransmitting{Agn} | "Requesting repeat..." | Yellow |
| UserTransmitting{Tu} | "Sending TU..." | Yellow |
| WaitingForStation (correction) | "Waiting for correction..." | LightBlue |
| WaitingForStation (normal) | "Waiting for response..." | LightBlue |
| StationTransmitting{SendingExchange} | "Receiving exchange - press Enter to log" | Green |
| StationTransmitting{RequestingAgn} | "Station requests repeat - press F2" | Orange |
| StationTransmitting{Correction} | "Station correcting callsign..." | Orange |
| QsoComplete | "QSO logged! Press F1 for next" | Green |
| WaitingForTailEnder | "QSO logged! Waiting..." | Green |

**Color values:**
- Gray: `Color32::GRAY`
- Yellow: `Color32::YELLOW`
- LightBlue: `Color32::LIGHT_BLUE`
- Green: `rgb(100, 200, 100)`
- Orange: `rgb(255, 165, 0)`

---

## 5. Keyboard Controls

| Key | Action | Condition |
|-----|--------|-----------|
| F1 | Stop all audio, send CQ, restart callers, clear inputs | Settings valid |
| F2 | Send exchange only | Active callers exist |
| F3 | Send TU | Always |
| F5 | Send his call (callsign field contents) | Active callers exist, non-empty callsign |
| F8 | Request repeat (AGN) | Context-dependent (callsign or exchange) |
| F12 | Wipe (clear all input fields) | Always |
| Enter | Submit current field / Send CQ if callsign empty | Depends on field |
| Tab / Shift+Tab | Navigate fields forward / backward | Always |
| Space / Shift+Space | Navigate fields forward / backward | Always |
| Up Arrow | Increase WPM by 1 | WPM < 50 |
| Down Arrow | Decrease WPM by 1 | WPM > 15 |
| Escape | Stop all audio transmission | Always |

### WPM Limits
- Minimum: 15 WPM
- Maximum: 50 WPM

### Field Navigation Order
- Callsign -> Exchange(0) -> Exchange(1) -> ... -> Exchange(N) -> Callsign (wraps)
- Backward: reverse of above

### Enter Key Behavior
- **Callsign field, empty input**: Acts as F1 (send CQ).
- **Callsign field, non-empty input**: Submit callsign, select matching caller, send exchange.
- **Exchange field**: Validate and log QSO, send TU.

---

## 6. Audio System

### 6.1 Audio Engine

- Uses `cpal` to open the system default output device.
- Discovers the device's native sample rate and uses it (overriding configured value).
- Supports sample formats: F32, I16, U16.
- Renders mono audio and duplicates to all output channels.
- Processes commands non-blocking from the command channel.

### 6.2 Audio Commands

| Command | Payload |
|---------|---------|
| StartStation | StationParams (id, callsign, exchange, freq_offset, wpm, amplitude) |
| PlayUserMessageSegmented | segments: Vec<MessageSegment>, wpm: u8 |
| UpdateSettings | AudioSettings |
| StopAll | (stops all audio except noise) |

### 6.3 Audio Events

| Event | Payload |
|-------|---------|
| StationComplete | StationId |
| UserMessageComplete | (none) |
| UserSegmentComplete | MessageSegmentType |

### 6.4 Message Segments

```
MessageSegmentType
  TheirCallsign   -- The caller's callsign
  OurExchange     -- Our exchange
  Cq              -- CQ call
  Tu              -- Thank you
  Agn             -- AGN/? request
```

### 6.5 Message Construction

| Message | Format | Segment Types |
|---------|--------|---------------|
| CQ | `"{cq_prefix} {user_callsign}"` | [Cq] |
| Exchange | `"{their_call}"` + `"{exchange}"` | [TheirCallsign, OurExchange] |
| Exchange Only | `"{exchange}"` | [OurExchange] |
| TU | `"TU {user_callsign}"` | [Tu] |
| His Call | `"{their_call}"` | [TheirCallsign] |
| AGN | `"{agn_message}"` (default "?") | [Agn] |

### 6.6 Morse Code Encoding

#### Character Table

Supports 41 characters: A-Z, 0-9, `/`, `?`, `.`, `,`, `=` (BT prosign).

#### Element Types and Durations (PARIS standard)

| Element | Units | Produces Tone |
|---------|-------|---------------|
| Dit | 1 | Yes |
| Dah | 3 | Yes |
| ElementGap | 1 | No |
| CharGap | 3 | No |
| WordGap | 7 | No |

#### Timing Formula

```
units_per_second = (wpm * 50) / 60
samples_per_unit = sample_rate / units_per_second
```

At 20 WPM / 44100 Hz: dit = 2646 samples (~60ms).

#### Text-to-Morse Serialization

1. Split on whitespace into words.
2. Each character -> Morse elements with `ElementGap` between elements.
3. `CharGap` between characters within a word.
4. `WordGap` between words.
5. Unknown characters are silently skipped.

### 6.7 Tone Generation

#### Sine Wave

```
sample = sin(phase * 2 * PI)
phase += frequency_hz / sample_rate      (phase wraps at 1.0)
```

Phase stored as f64 for precision; output is f32.

#### Raised Cosine Keying Envelope

Ramp time: 5ms (`ramp_samples = sample_rate * 0.005`).

- **Attack** (0 to ramp_samples): `0.5 * (1.0 - cos(PI * position / ramp_samples))`
- **Sustain**: `1.0`
- **Release** (last ramp_samples): `0.5 * (1.0 + cos(PI * release_pos / ramp_samples))`

#### Station Signal Formula

```
sample = sin(phase) * envelope(elapsed, total) * amplitude * qsb_factor
```

Station tone frequency = `center_frequency + frequency_offset_hz`.

#### User Signal Formula

```
sample = sin(phase) * envelope(elapsed, total) * 0.8
```

User amplitude is hardcoded at **0.8**. No QSB applied. User plays at center frequency (no offset).

### 6.8 QSB (Fading) Simulation

Three-layer sinusoidal oscillator per station:

```
base_angular_velocity = rate * 2 * PI / 60.0 / sample_rate
```

| Layer | Velocity Range | Weight |
|-------|---------------|--------|
| 0 | base * [0.9, 1.1] | 0.5 |
| 1 | base * [0.6, 0.8] | 0.3 |
| 2 | base * [1.2, 1.4] | 0.2 |

Each layer starts with random initial phase in [0, 2*PI).

```
combined = 0.5 * sin(phase[0]) + 0.3 * sin(phase[1]) + 0.2 * sin(phase[2])
normalized = (combined + 1.0) / 2.0
factor = 1.0 - depth + depth * normalized
```

At depth=0: factor is always 1.0. At depth=1: factor ranges [0.0, 1.0].

QSB oscillator advances on every sample (tone and gap), keeping fading continuous.

### 6.9 Noise Generation

#### Pink Noise (Kellet-style 1/f approximation)

Three-pole IIR filter on white noise:

```
b0 = 0.99765 * b0 + white * 0.0990460
b1 = 0.96300 * b1 + white * 0.2965164
b2 = 0.57000 * b2 + white * 1.0526913
output = (b0 + b1 + b2 + white * 0.1848) * 0.05
```

#### Bandpass Filter (Biquad)

Standard Audio EQ Cookbook bandpass (constant 0 dB peak gain):

```
omega = 2 * PI * center_freq / sample_rate
Q = center_freq / bandwidth
alpha = sin(omega) / (2 * Q)
b0 = alpha, b1 = 0, b2 = -alpha
a0 = 1 + alpha, a1 = -2*cos(omega), a2 = 1 - alpha
```

All coefficients normalized by a0. Default: center = 600 Hz, bandwidth = 350-400 Hz.

#### Static Crashes

- **Trigger probability**: `crash_rate / sample_rate` per sample (only when no crash active).
- **Duration**: Random 50-200ms.
- **Amplitude**: `crash_intensity * random(0.5..1.0)`.
- **Decay**: Exponential to 10% by end: `decay = 0.1^(1.0 / total_samples)`.
- **Per-sample**: `noise = random(-1..1) * crash_amplitude; crash_amplitude *= decay`.

#### Pops/Clicks

- **Trigger probability**: `pop_rate / sample_rate` per sample (only when no pop active).
- **Duration**: Random 1-5ms.
- **Amplitude**: `pop_intensity * random(0.6..1.0)`, 50% chance of negated polarity.
- **Per-sample**: `progress = 1.0 - min(remaining / 5.0, 1.0); sample = amplitude * (1.0 - progress)`.

#### QRN (Atmospheric Rumble)

```
base_freq = 0.3 Hz
osc1 = sin(phase)
osc2 = sin(phase * 1.7)
mod_depth = 0.5 + 0.5 * sin(mod_phase)         (modulation at 0.03 Hz)
noise = random(-0.3..0.3)
output = qrn_intensity * mod_depth * (osc1 * 0.6 + osc2 * 0.3 + noise)
```

QRN bypasses the bandpass filter.

#### Noise Floor Modulation

```
noise_floor_alpha = 1.0 - exp(-2 * PI * 0.3 / sample_rate)
noise_floor += noise_floor_alpha * (random_target - noise_floor)
floor_mod = 1.0 + qrn_intensity * 0.4 * noise_floor
```

#### Per-Sample Noise Pipeline

1. Check and maybe start crash/pop events.
2. Generate white noise -> pink coloring.
3. Add crash + pop samples.
4. Bandpass filter (pink + crash + pop).
5. Apply noise floor modulation.
6. Add QRN (unfiltered).
7. Scale by noise_level.

Noise is additive to the output buffer.

### 6.10 Mixer

#### Master Mix Pipeline (per buffer fill)

1. Zero-fill buffer.
2. Determine muting: `mute_rx = mute_rx_during_tx AND user_transmitting`, `mute_sidetone = mute_sidetone_during_tx AND user_transmitting`.
3. Add noise (skipped if `mute_rx`).
4. Mix all station signals additively (skipped if `mute_rx`).
5. Mix user signal additively (skipped if `mute_sidetone`).
6. Per-sample post-processing:
   - **Master volume**: `sample *= master_volume`
   - **Triangular dither**: `sample += (random(0..1) - 0.5) * 0.001`
   - **Soft clipping** (above 0.8): `sample = sign * (0.8 + 0.2 * tanh(|sample| - 0.8))`

---

## 7. Configuration

### 7.1 Settings File Location

| Platform | Path |
|----------|------|
| Linux | `~/.config/contest_trainer/settings.toml` |
| macOS | `~/Library/Application Support/contest_trainer/settings.toml` |
| Windows | `%APPDATA%\contest_trainer\settings.toml` |
| Fallback | `settings.toml` in current working directory |

### 7.2 Settings Load/Save Behavior

- On load failure: if file exists, create backup as `"{filename}.bak.{unix_timestamp}"`, use defaults, show notice.
- On save: serialize to TOML pretty format, create parent directories as needed.
- Contest-specific settings are stored under `contest.contests.<contest_id>` in the TOML file.
- Missing contest settings keys are merged with defaults via `merge_defaults`.

### 7.3 User Settings

| Field | Type | Default | Range |
|-------|------|---------|-------|
| callsign | String | "N9UNX" | N/A |
| wpm | u8 | 32 | 15-50 |
| font_size | f32 | 14.0 | 10.0-24.0 |
| agn_message | String | "?" | N/A |
| show_main_hints | bool | false | N/A |
| show_status_line | bool | true | N/A |

### 7.4 Audio Settings

| Field | Type | Default | UI Range |
|-------|------|---------|----------|
| sample_rate | u32 | 44100 | Not in UI (overridden by device) |
| tone_frequency_hz | f32 | 600.0 | 400.0-1000.0 |
| noise_level | f32 | 0.25 | 0.0-0.5 |
| master_volume | f32 | 0.7 | 0.0-1.0 |
| mute_rx_during_tx | bool | true | N/A |
| mute_sidetone_during_tx | bool | false | N/A |
| noise_bandwidth | f32 | 350.0 | 100.0-1000.0 |

#### Noise Settings

| Field | Type | Default | UI Range |
|-------|------|---------|----------|
| crash_rate | f32 | 0.4 | 0.0-2.0 (per second) |
| crash_intensity | f32 | 0.2 | 0.0-1.0 |
| pop_rate | f32 | 0.6 | 0.0-10.0 (per second) |
| pop_intensity | f32 | 0.73 | 0.0-1.0 |
| qrn_intensity | f32 | 0.3 | 0.0-1.0 |

#### QSB Settings

| Field | Type | Default | UI Range |
|-------|------|---------|----------|
| enabled | bool | false | N/A |
| depth | f32 | 0.5 | 0.0-1.0 |
| rate | f32 | 4.0 | 1.0-20.0 (cycles per minute) |

### 7.5 Simulation Settings

| Field | Type | Default | UI Range |
|-------|------|---------|----------|
| max_simultaneous_stations | u8 | 2 | 1-5 |
| station_probability | f32 | 0.7 | 0.1-1.0 |
| wpm_min | u8 | 28 | 10-50 |
| wpm_max | u8 | 36 | 10-50 |
| frequency_spread_hz | f32 | 300.0 | 100.0-500.0 |
| amplitude_min | f32 | 0.4 | 0.1-1.0 |
| amplitude_max | f32 | 1.0 | 0.1-1.0 |
| agn_request_probability | f32 | 0.1 | 0.0-1.0 |
| same_country_filter_enabled | bool | false | N/A |
| same_country_probability | f32 | 0.1 | 0.0-1.0 |

#### Pileup Settings (not exposed in UI)

| Field | Type | Default |
|-------|------|---------|
| min_patience | u8 | 2 |
| max_patience | u8 | 5 |
| retry_delay_min_ms | u32 | 200 |
| retry_delay_max_ms | u32 | 1200 |

#### Call Correction Settings (not exposed in UI)

| Field | Type | Default |
|-------|------|---------|
| correction_probability | f32 | 0.8 |
| max_correction_attempts | u8 | 2 |

---

## 8. Caller Management

### 8.1 Persistent Caller Queue

Callers persist across CQ cycles. Each caller has:

| Property | Type | Range/Default |
|----------|------|---------------|
| params | StationParams | (see below) |
| patience | u8 | random(min_patience..=max_patience), default 2-5 |
| attempts | u8 | Starts at 0 |
| state | CallerState | Waiting, Calling, GaveUp, Worked |
| reaction_delay_ms | u32 | random(100..800) ms |
| ready_at | Instant | When caller is ready to try again |

### 8.2 Station Parameters

| Field | Type | Generation |
|-------|------|------------|
| id | StationId(u32) | Monotonically increasing |
| callsign | String | From callsign source |
| exchange | Exchange | From contest generator |
| frequency_offset_hz | f32 | random(-spread/2..spread/2), default +/-150 Hz |
| wpm | u8 | random(wpm_min..=wpm_max), default 28-36 |
| amplitude | f32 | random(amplitude_min..amplitude_max), default 0.4..1.0 |

### 8.3 Queue Replenishment

- **Rate limiting**: Maximum once per 500ms.
- **Target queue size**: `ceil(max_simultaneous_stations * 2.5)` (default: 5).
- **Probability gate**: Each new caller addition rolls against `station_probability` (default 0.7); on failure, stops adding.
- **Country filtering**: Up to 10 retries per caller. If `same_country_filter_enabled`, callers from the user's country are rejected with probability `(1.0 - same_country_probability)`.

### 8.4 CQ Response Selection (on_cq_complete)

1. Remove all Worked and GaveUp callers.
2. Sort remaining by `reaction_delay_ms + random_jitter(0..100)`.
3. For each caller up to `max_simultaneous_stations`:
   - Must be in Waiting state.
   - **Call probability**: `0.5 + (patience - 1) * 0.1` (patience 2 = 60%, patience 5 = 90%).
   - On success: mark as Calling, record attempt, add to active list.

### 8.5 CQ Restart (on_cq_restart)

When user presses F1 without completing a QSO:
- Callers in Calling state who have exceeded patience -> GaveUp.
- Others -> set retry delay (random 200-1200ms), state back to Waiting.
- Clear active list.

### 8.6 Tail-Ender Spawning (try_spawn_tail_ender)

1. Replenish queue, remove Worked/GaveUp.
2. Sort waiting callers by reaction delay with random jitter (same as CQ).
3. Loop up to `max_simultaneous_stations`, selecting waiting+ready callers.
4. **Geometric decay per slot**: `base_probability * 0.5^slot` where:
   - `base_probability = 0.5 + (patience - 1) * 0.1` (same formula as CQ, 60-90%)
   - `slot` = 0-indexed count of callers already selected
   - Decay factor `0.5` is hardcoded (no config setting)
5. Expected outcome: ~60-90% chance of 1 tail-ender, ~30-45% for 2, ~15% for 3, rare beyond that.

---

## 9. Contest System

### 9.1 Contest Plugin Architecture

Contests are auto-discovered at build time. Any `.rs` file in `src/contest/` (excluding `mod.rs`, `types.rs`, `callsign.rs`) that exports these items is automatically registered:

```rust
pub const CONTEST_ID: &str;
pub const DISPLAY_NAME: &str;
pub fn make_contest() -> Box<dyn Contest>;
```

Modules are sorted alphabetically, determining registry order.

### 9.2 Contest Trait

Required methods:

| Method | Purpose |
|--------|---------|
| `id()` | Unique string identifier |
| `display_name()` | Human-readable name |
| `exchange_fields()` | Define user input fields |
| `settings_fields()` | Define settings UI schema |
| `default_settings()` | Default TOML settings |
| `validate_settings()` | Validate settings (optional, default no-op) |
| `cq_message()` | CQ message text |
| `callsign_source()` | Create callsign source |
| `generate_exchange()` | Generate exchange for a caller |
| `format_exchange()` | Format exchange for Morse (default: join with spaces) |
| `user_exchange_fields()` | User's exchange as field list |
| `format_user_exchange()` | Format user exchange for Morse (default: join with spaces) |
| `validate()` | Validate user's logged entry |
| `format_received_exchange()` | Format for display (default: join with spaces) |

### 9.3 Exchange Field Definition

| Property | Type | Purpose |
|----------|------|---------|
| label | &str | Display label |
| placeholder | &str | Hint text |
| width_chars | u8 | UI width hint |
| kind | FieldKind | Text, Number, Alnum, or Section |
| default_value | Option<&str> | Pre-filled value (e.g., "5NN") |
| focus_on_enter | bool | Receives focus after callsign entry |

#### Input Normalization

1. Trim whitespace.
2. Convert to uppercase.
3. Remove internal whitespace.
4. If kind is Number: additionally strip all non-digit characters.

### 9.4 Callsign Validation Rules (shared)

- Length: 3 to 10 characters inclusive.
- Must contain at least one ASCII letter.
- Must contain at least one ASCII digit.
- All characters must be ASCII alphanumeric or `/`.

### 9.5 Default Callsign Pool (46 callsigns)

Used as fallback when no callsign file is loaded:

```
W1AW, K1TTT, N1MM, W2FU, K2LE, N2IC, W3LPL, K3LR, N3RS, W4MYA,
K4JA, N4AF, W5WMU, K5ZD, N5TJ, W6YX, K6XX, N6TV, W7RN, K7RL,
N7DR, W8ND, K8ND, N8II, W9RE, K9CT, N9RV, W0AIH, K0RF, N0AX,
VE3EJ, VE7CC, VA3DX, VE2IM, VE6SV, DL1A, DL6FBL, G3PXT, G4AMJ,
JA1ABC, JH1NBN, PY2SEX, LU1FAM, ZS6EZ, VK2GR, ZL1BQD
```

Selection: random from unused; when all used, reset and pick randomly.

### 9.6 RST Generation (shared by CQWW, CQWPX, ARRL DX)

Random roll 0-99:
- 0-4 (5%): `"ENN"`
- 5-14 (10%): `"599"`
- 15-99 (85%): `"5NN"`

#### RST Normalization

| Input | Normalized |
|-------|-----------|
| E | 5 |
| N | 9 |
| T | 0 |

### 9.7 Serial Number Formatting (shared by CQWPX, Sweepstakes)

- Serial < 100: zero-pad to 3 digits (e.g., 1 -> "001").
- Serial >= 100: no padding (e.g., 1234 -> "1234").

#### CW Digit Normalization (for serial parsing)

| Input | Normalized |
|-------|-----------|
| T | 0 |
| N | 9 |

---

## 10. Contest Definitions

### 10.1 CWT (CW Ops CW Test)

| Property | Value |
|----------|-------|
| ID | `"cwt"` |
| Display Name | `"CWT"` |
| Points per QSO | 1 |

#### Exchange Fields

| # | Label | Placeholder | Width | Kind |
|---|-------|-------------|-------|------|
| 0 | Name | BOB | 8 | Text |
| 1 | Number | 123 | 6 | Alnum |

#### Settings

| Key | Label | Default | Group |
|-----|-------|---------|-------|
| cq_message | CQ Message | "CQ TEST" | Contest |
| callsign_file | CWT Callsign File | "cwt_callsigns.txt" | Contest |
| user_name | Your Name | "OP" | UserExchange |
| user_number | Your Number | "CT" | UserExchange |

#### CWT Callsign File Format (CSV)

```
# callsign, name, number
W1AW, JOE, 1
```

- Fields: callsign, name, number (all required, non-empty).
- Lines starting with `#` or `!` are comments.
- All values uppercased.
- Callsign must pass validation rules.

#### Default CWT Station Pool

| Callsign | Name | Number |
|----------|------|--------|
| W1AW | JOE | 1 |
| K5ZD | RANDY | 2 |
| N1MM | TOM | 100 |
| K3LR | TIM | 55 |
| W9RE | MIKE | IN |

#### Validation

- Callsign: case-insensitive exact match.
- Name (field 0): case-insensitive exact match.
- Number (field 1): case-insensitive exact match.

---

### 10.2 CQ World Wide (CQWW)

| Property | Value |
|----------|-------|
| ID | `"cqww"` |
| Display Name | `"CQ World Wide"` |
| Points per QSO | 1 |

#### Exchange Fields

| # | Label | Placeholder | Width | Kind | Default |
|---|-------|-------------|-------|------|---------|
| 0 | RST | 5NN | 3 | Text | "5NN" |
| 1 | Zone | 05 | 2 | Number | (none, focus_on_enter) |

#### Settings

| Key | Label | Default | Group |
|-----|-------|---------|-------|
| cq_message | CQ Message | "CQ TEST" | Contest |
| callsign_file | Callsign File | "callsigns.txt" | Contest |
| user_zone | Your Zone | "05" | UserExchange |

#### CQ Zone Determination

Uses embedded `cty.dat` database to look up CQ zone by callsign prefix. Fallback zone: 5.

#### Validation

- Callsign: case-insensitive exact match.
- RST: both sides normalized, compared as strings.
- Zone: both parsed as u8, compared numerically.

---

### 10.3 CQ WPX

| Property | Value |
|----------|-------|
| ID | `"cqwpx"` |
| Display Name | `"CQ WPX"` |
| Points per QSO | 1 |

#### Constants

- SERIAL_MIN_DEFAULT: 1000
- SERIAL_MAX_DEFAULT: 2500
- SERIAL_MIN_ALLOWED: 1
- SERIAL_MAX_ALLOWED: 12000

#### Exchange Fields

| # | Label | Placeholder | Width | Kind | Default |
|---|-------|-------------|-------|------|---------|
| 0 | RST | 5NN | 3 | Text | "5NN" |
| 1 | SER | SER | 5 | Alnum | (none, focus_on_enter) |

#### Settings

| Key | Label | Default | Kind | Group |
|-----|-------|---------|------|-------|
| cq_message | CQ Message | "CQ TEST" | Text | Contest |
| callsign_file | Callsign File | "callsigns.txt" | FilePath | Contest |
| serial_min | Serial Min | 1000 | Integer{1..12000} | Contest |
| serial_max | Serial Max | 2500 | Integer{1..12000} | Contest |

#### Validation

- Callsign: case-insensitive exact match.
- RST: both sides normalized, compared as strings.
- Serial: both sides parsed (T->0, N->9 normalization), compared numerically as u32.
- Settings: serial_min must be <= serial_max; both clamped to [1, 12000].

---

### 10.4 ARRL Sweepstakes

| Property | Value |
|----------|-------|
| ID | `"sweepstakes"` |
| Display Name | `"ARRL Sweepstakes"` |
| Points per QSO | 2 |

#### Constants

- PRECEDENCES: `['Q', 'A', 'B', 'U', 'M', 'S']`
- SERIAL_MIN_DEFAULT: 100
- SERIAL_MAX_DEFAULT: 400
- SERIAL_MIN_ALLOWED: 1
- SERIAL_MAX_ALLOWED: 12000

#### Exchange Fields (user entry)

| # | Label | Placeholder | Width | Kind |
|---|-------|-------------|-------|------|
| 0 | NR | 001 | 4 | Number |
| 1 | P | A | 1 | Text |
| 2 | CK | 99 | 2 | Number |
| 3 | Sec | CT | 3 | Section |

Internal exchange has 5 fields: `[serial, precedence, callsign, check, section]`. User logs only 4 (callsign is separate).

#### Settings

| Key | Label | Default | Kind | Group |
|-----|-------|---------|------|-------|
| cq_message | CQ Message | "CQ TEST" | Text | Contest |
| callsign_file | SS Callsign File | "ss_callsigns.txt" | FilePath | Contest |
| serial_min | Serial Min | 100 | Integer{1..12000} | Contest |
| serial_max | Serial Max | 400 | Integer{1..12000} | Contest |
| user_precedence | Your Precedence | "A" | Text | UserExchange |
| user_check | Your Check | "99" | Text | UserExchange |
| user_section | Your Section | "CT" | Text | UserExchange |

#### Sweepstakes Callsign File Format (CSV)

```
# Call, Sect, State, CK, UserText
K1ABC, CT, , 94,
```

- Field 0: callsign, Field 1: section, Field 2: (ignored), Field 3: check.
- Minimum 4 fields required.
- Check must be all ASCII digits.

#### Default SS Station Pool

| Callsign | Section | Check |
|----------|---------|-------|
| W1AW | CT | 38 |
| K5ZD | EMA | 90 |
| N0AX | WCF | 72 |
| K3LR | WPA | 79 |

#### Section-from-Callsign Fallback Mapping

| Call Area | Section |
|-----------|---------|
| 1 | CT |
| 2 | NNJ |
| 3 | EPA |
| 4 | VA |
| 5 | NTX |
| 6 | SDG |
| 7 | OR |
| 8 | OH |
| 9 | IL |
| 0 | CO |
| (none) | SDG |

#### Exchange Generation

- Serial: random in configured range, formatted per serial rules.
- Precedence: random from `['Q', 'A', 'B', 'U', 'M', 'S']`.
- Check: from file or random 60-99, formatted as 2-digit zero-padded.
- Section: from file or derived from callsign.

#### Validation

- Callsign: case-insensitive exact match.
- NR (received[0] vs expected[0]): parsed as serial (T->0, N->9), compared numerically.
- P (received[1] vs expected[1]): first character uppercased comparison.
- CK (received[2] vs expected[3]): parsed as u16, compared numerically.
- Sec (received[3] vs expected[4]): uppercased string comparison.

---

### 10.5 ARRL DX CW

| Property | Value |
|----------|-------|
| ID | `"arrldx"` |
| Display Name | `"ARRL DX CW"` |
| Points per QSO | 1 |

#### Exchange Fields

| # | Label | Placeholder | Width | Kind | Default |
|---|-------|-------------|-------|------|---------|
| 0 | RST | 5NN | 3 | Text | "5NN" |
| 1 | Exchange | ST/PWR | 6 | Alnum | (none, focus_on_enter) |

#### Settings

| Key | Label | Default | Group |
|-----|-------|---------|-------|
| cq_message | CQ Message | "CQ TEST" | Contest |
| callsign_file | Callsign File | "arrldx_callsigns.txt" | Contest |
| user_exchange | Your Exchange | "CT" | UserExchange |

#### ARRL DX Callsign File Format (CSV)

```
# Call, Name, State, Power, UserText
VE2FK, , QC, ,
DL1ABC, , , 100,
```

- Minimum 4 fields per line.
- **Exclusive-or rule**: exactly one of state (field 2) or power (field 3) must be non-empty.
- The non-empty field becomes the exchange value.

#### Default ARRL DX Station Pool

| Callsign | Exchange | Type |
|----------|----------|------|
| VE2FK | QC | State/Province |
| K3LR | PA | State |
| DL1ABC | 100 | Power |
| JA1ABC | 500 | Power |

#### Power T-Substitution

For all-digit exchange values (power):
- 80% chance: replace all `0` with `T` (e.g., 100 -> 1TT, 500 -> 5TT).
- 20% chance: send as-is.
- Non-numeric exchanges (state abbreviations) are unchanged.

#### Validation

- Callsign: case-insensitive exact match.
- RST: both sides normalized (E->5, N->9, T->0), compared.
- Exchange: both sides normalized (T->0, N->9), compared as strings.

---

## 11. Country Data (cty.dat)

### 11.1 Purpose

The `cty.dat` file (embedded at compile time) provides DXCC entity/prefix data for:
- Looking up CQ zones by callsign.
- Determining if two callsigns are from the same country.
- Prefix-to-country mapping.

### 11.2 File Format

**Header lines** (detected by having >= 7 colons and not ending with `;` or `,`):
```
Country Name:  CQ:  ITU:  Cont:  Lat:  Lon:  TZ:  Prefix:
```

CQ zone is parsed from field index 1. Primary prefix from field index 7 (leading `*` stripped).

**Alias lines** follow header lines, ending with `;`:
- Comma-separated list of prefixes/callsigns.
- Leading `=` means exact callsign match.
- `(N)` overrides CQ zone; `[N]` overrides ITU zone.
- `{...}`, `<...>`, `~...~` are ignored (continent, lat/lon, timezone).

### 11.3 Lookup Priority

1. Exact callsign match (highest priority).
2. Longest prefix match (prefixes sorted by length descending).

---

## 12. Build System

### 12.1 Build Script (build.rs)

The build script auto-discovers contest modules:

1. Scans `src/contest/` for `.rs` files.
2. Excludes `mod.rs`, `types.rs`, `callsign.rs`.
3. Sorts remaining modules alphabetically.
4. Generates `contest_registry.rs` containing:
   - Module declarations with absolute paths.
   - A `generated_contest_registry()` function returning `Vec<ContestDescriptor>`.
5. Each contest module must export `CONTEST_ID`, `DISPLAY_NAME`, and `make_contest()`.

### 12.2 Registry Order

Alphabetical by filename: `arrldx`, `cqwpx`, `cqww`, `cwt`, `sweepstakes`.

The default contest is the first in the registry (currently `arrldx`), with a fallback to `"cwt"` if lookup fails.

---

## 13. User Interface

### 13.1 Main Panel Layout (top to bottom)

1. **Contest type display**: Bold "Contest:" label + contest display name.
2. **Settings notice** (conditional): Yellow text with "Dismiss" button.
3. **Score bar**: QSOs | Points | Rate (/hr) | Run WPM.
4. **Separator**
5. **Status line** (conditional on `show_status_line`): Bold "Status:" + colored status text.
6. **Input fields grid**: Callsign field (120x24 px, monospace) + exchange fields (width calculated from `font_size * 0.6 * width_chars + 8.0`, height 24px, monospace).
7. **Separator**
8. **Function key hints**: F1-CQ, F2-Exchange, F3-TU, F5-His Call, F8-?, F12-Wipe, Enter-Submit, Esc-Stop.
9. **Last QSO result** (conditional): Callsign + Call OK/X (green/red) + Exch OK/X (green/red) + points. Expected values shown if incorrect.
10. **Separator**
11. **Bottom buttons**: Reset Stats | Toggle Static (ON/OFF) | Session Stats.

### 13.2 Settings Window

**Viewport**: Separate window, "Settings", 475 x 600 px.

**Sections (collapsible headers):**

1. **User Settings** (default open): Callsign, WPM slider (15-50), Font Size slider (10-24), AGN Message, Show Status Line checkbox, Show Main Field Hints checkbox.
2. **Contest Settings** (default open): Contest Type combo box.
3. **Active Contest** (default open): Dynamically rendered from contest's `settings_fields()`, split into Contest and Your Exchange groups.
4. **Simulation Settings** (default open): Max stations (1-5), Station probability (0.1-1.0), WPM range (10-50), Filter width (100-500 Hz), Signal strength range (0.1-1.0), Caller needs repeat probability (0.0-1.0), Country filter checkbox + same-country probability.
5. **Audio Settings** (default closed): Tone frequency (400-1000 Hz), Noise level (0.0-0.5), Noise bandwidth (100-1000 Hz), Master volume (0.0-1.0), Mute RX checkbox, Mute sidetone checkbox. Sub-sections for Static/QRN (crash rate, crash intensity, pop rate, pop intensity, QRN intensity) and QSB (enable checkbox, fade depth, fade rate).

### 13.3 Statistics Window

**Viewport**: Separate window, "Session Statistics", 450 x 550 px.

**Top bar**: "Export Stats" button.

**Sections (in ScrollArea):**

1. **Session Summary**: Total QSOs, Correct QSOs (with %), Total Points.
2. **Accuracy**: Callsign accuracy (n/total, %), Exchange accuracy (n/total, %).
3. **Streaks**: Current/Max Clean, Current/Max Error. ("Clean = callsign and exchange both correct").
4. **F5/F8 Usage**: F5 count, F8 Callsign count, F8 Exchange count, Total with F8 (with %).
5. **Calling Station Speed**: Average WPM, WPM range.
6. **WPM Accuracy (2-WPM buckets)**: Table with Bucket, Total, Correct, Accuracy%.
7. **Character Error Analysis**: Table with Char, Error Rate%, Samples. Top 10 chars with >0% error rate, minimum 3 samples. Space displayed as "[space]".
8. **Recent QSOs**: Last 15 QSOs in reverse chronological order. Table with Callsign (green/red), Exchange (green/red), WPM, AGN (C/X in yellow or "-"), Result (OK green / ok light-green / ERR red).

**Result column logic:**
- `"OK"` (green): Correct AND no AGN/F5 used.
- `"ok"` (light green): Correct but AGN or F5 was used.
- `"ERR"` (red): Callsign or exchange wrong.

### 13.4 Export Dialog

Modal window, "Export Complete", centered, not collapsible/resizable. Shows filename in monospace bold with "OK" button.

### 13.5 Toggle Noise Behavior

- If enabled: save current noise level, set to 0.0, mark disabled.
- If disabled: restore saved level (fallback to 0.15 if saved was 0.0), mark enabled.

---

## 14. Statistics System

### 14.1 QSO Record

Each logged QSO records:

| Field | Type |
|-------|------|
| expected_callsign | String |
| entered_callsign | String |
| callsign_correct | bool |
| expected_exchange | String |
| entered_exchange | String |
| exchange_correct | bool |
| station_wpm | u8 |
| points | u32 |
| used_agn_callsign | bool |
| used_agn_exchange | bool |
| used_f5_callsign | bool |

### 14.2 Analysis Calculations

- **Callsign accuracy**: `(correct_callsigns / total_qsos) * 100.0`
- **Exchange accuracy**: `(correct_exchanges / total_qsos) * 100.0`
- **Correct rate**: `(correct_qsos / total_qsos) * 100.0` (correct = both callsign AND exchange correct)
- **Hourly rate**: `qso_count / elapsed_hours` (returns 0 if elapsed < 0.01 hours)

### 14.3 WPM Buckets

- Bucket size: 2 WPM.
- Bucket start: `(station_wpm / 2) * 2` (integer division).
- Label: `"{start}-{start+1}"` (e.g., "28-29").
- Accuracy: both callsign and exchange correct.
- Sorted ascending by WPM.

### 14.4 Streak Tracking

- **Clean QSO**: Both callsign AND exchange correct.
- Tracks current and max streaks for both clean and error runs.
- Iterates QSOs in order; clean increments clean streak and resets error streak; vice versa.

### 14.5 Character Error Analysis

- Counts every alphanumeric character (uppercased) in expected callsigns and exchanges.
- Errors counted positionally: for each position in expected, if entered character at same index differs or is missing, count as error for that expected character.
- Error rate: `(errors / total) * 100.0` per character.
- **Minimum sample filter**: Only characters with total >= 3 are included.
- Sorted: descending by error rate, then ascending by character.

---

## 15. Session Export

### 15.1 File Naming

```
CWCT-{CALLSIGN}-{YYYYMMDD}-{HHMM}.md
```

- Callsign uppercased, trimmed; empty becomes "NOCALL".
- Created in the current working directory.

### 15.2 Markdown Structure

1. **Header**: Title, callsign, export timestamp.
2. **Session Summary**: Total QSOs, correct QSOs (%), total points.
3. **Accuracy**: Callsign and exchange accuracy (n/total, %).
4. **Streaks**: Current/max clean and error streaks.
5. **F5/F8 Usage**: Counts and percentages.
6. **Calling Station Speed**: Average WPM, range.
7. **WPM Accuracy**: Table of 2-WPM buckets with total/correct/accuracy%.
8. **Character Error Analysis**: Top 10 characters with errors, min 3 samples.
9. **QSO Log**: Full table with expected/entered callsign/exchange, correctness, WPM, points, AGN/F5 usage.

---

## 16. Data Files

### 16.1 Callsign Files

| File | Format | Contest |
|------|--------|---------|
| callsigns.txt | One callsign per line (or CSV, first field) | CQWW, CQWPX |
| cwt_callsigns.txt | CSV: callsign, name, number | CWT |
| arrldx_callsigns.txt | CSV: call, (ignored), state, power | ARRL DX |
| ss_callsigns.txt | CSV: call, section, (ignored), check | Sweepstakes |

All files: lines starting with `#` or `!` are comments; empty lines skipped; values uppercased.

### 16.2 Country Data

- `data/cty.dat`: DXCC prefix database, embedded at compile time via `include_str!`.
- Source: Amateur Radio Country Files (AD1C).

---

## 17. Timing Constants Summary

| Delay | Duration | Purpose |
|-------|----------|---------|
| Post-CQ delay | 300ms | Before callers start responding |
| Post-user-TX delay | 250ms | Before station responds to user |
| Tail-ender delay | 100ms | Before tail-ender starts calling |
| Caller retry delay | 200-1200ms | Before persistent caller tries again |
| Caller reaction time | 100-800ms | How fast a caller responds to CQ |
| Queue replenish cooldown | 500ms | Minimum time between queue replenishments |
| Morse ramp time | 5ms | Raised cosine keying envelope rise/fall |

---

## 18. Score System

### 18.1 QSO Result

| Field | Type |
|-------|------|
| callsign | String (what user entered) |
| expected_call | String |
| expected_exchange | String |
| callsign_correct | bool |
| exchange_correct | bool |
| points | u32 |

### 18.2 Score Tracking

| Field | Type | Initial |
|-------|------|---------|
| qso_count | u32 | 0 |
| total_points | u32 | 0 |
| start_time | Option<Instant> | None (set on first QSO) |

- **Hourly rate**: `qso_count / (elapsed_seconds / 3600.0)`, returns 0 if elapsed < 0.01 hours.
- **User serial**: Starts at 1, incremented after each completed QSO.

### 18.3 Points Per Contest

| Contest | Points |
|---------|--------|
| CWT | 1 |
| CQ World Wide | 1 |
| CQ WPX | 1 |
| ARRL Sweepstakes | 2 |
| ARRL DX CW | 1 |

---

## 19. Release and Distribution

### 19.1 Build Targets

| Platform | Target |
|----------|--------|
| Linux | x86_64-unknown-linux-gnu |
| Windows | x86_64-pc-windows-msvc |
| macOS Intel | x86_64-apple-darwin |
| macOS ARM | aarch64-apple-darwin |

### 19.2 Release Bundle Contents

Each release zip contains a `contest_trainer/` folder with:
- The compiled binary.
- `callsigns.txt`
- `cwt_callsigns.txt`
- `arrldx_callsigns.txt`
- `ss_callsigns.txt`

Windows builds include Azure Trusted Signing for code signing.

### 19.3 License

MIT License.
