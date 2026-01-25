# Contest Trainer State Machine

This document describes the state machine that governs the contest simulation flow. It serves as both a developer reference and context for understanding the application behavior.

## Overview

The contest trainer simulates a CW (Morse code) contest QSO flow. The user calls CQ, stations respond, and the user exchanges information with them. The state machine manages all the transitions between these phases.

## Architecture

### Key Components

- **ContestState**: Enum defining all possible states
- **CallerManager**: Manages a persistent queue of callers with patience/retry behavior
- **ActiveCaller**: Wrapper around StationParams for a station currently in play
- **InputField**: Tracks whether user is in Callsign or Exchange field

### Audio Events

Two types of audio events drive state transitions:
- **UserMessageComplete**: User's transmission finished (CQ, exchange, TU, AGN)
- **StationComplete(id)**: A station finished transmitting

## States

### Idle States

| State | Description | Data |
|-------|-------------|------|
| `Idle` | Waiting for user to start | None |

### CQ Phase

| State | Description | Data |
|-------|-------------|------|
| `CallingCq` | User is sending CQ message | None |
| `WaitingForCallers` | CQ finished, 300ms delay before stations respond | None |

### Stations Calling Phase

| State | Description | Data |
|-------|-------------|------|
| `StationsCalling` | One or more stations are sending their callsigns | `callers: Vec<ActiveCaller>` |
| `QueryingPartial` | User sent partial callsign query (F5) | `callers`, `partial: String` |
| `WaitingForPartialResponse` | Brief pause before matching station repeats | `callers`, `wait_until: Instant` |
| `SendingCallsignAgn` | User requested callsign repeat (F8 in call field) | `callers: Vec<ActiveCaller>` |
| `WaitingForCallsignAgn` | Brief pause before station(s) repeat callsign | `callers`, `wait_until: Instant` |

### Exchange Phase

| State | Description | Data |
|-------|-------------|------|
| `SendingExchange` | User is sending their exchange to the station | `caller: ActiveCaller` |
| `WaitingToSendExchange` | 250ms pause before station sends exchange | `caller`, `wait_until: Instant` |
| `ReceivingExchange` | Station is sending their exchange | `caller: ActiveCaller` |
| `SendingAgn` | User requested exchange repeat (F8 in exchange field) | `caller: ActiveCaller` |
| `WaitingForAgn` | Brief pause before station resends exchange | `caller`, `wait_until: Instant` |

### Call Correction Phase

| State | Description | Data |
|-------|-------------|------|
| `SendingExchangeWillCorrect` | User sending exchange, but call was wrong - correction pending | `caller`, `correction_attempts` |
| `WaitingToSendCallCorrection` | Pause before station corrects wrong callsign | `caller`, `correction_attempts`, `wait_until` |
| `SendingCallCorrection` | Station is sending callsign correction | `caller`, `correction_attempts` |
| `WaitingForCallCorrection` | Waiting for user to fix callsign and resend | `caller`, `correction_attempts` |
| `SendingCallsignAgnFromCorrection` | User requested callsign repeat (F8) during correction | `caller`, `correction_attempts` |
| `WaitingForCallsignAgnFromCorrection` | Pause before station repeats callsign during correction | `caller`, `correction_attempts`, `wait_until` |
| `SendingCorrectionRepeat` | Station repeating callsign after F8/F5 during correction | `caller`, `correction_attempts` |
| `QueryingPartialFromCorrection` | User sent partial query (F5) during correction | `caller`, `correction_attempts` |
| `WaitingForPartialResponseFromCorrection` | Pause before station responds to partial during correction | `caller`, `correction_attempts`, `wait_until` |

### Caller AGN Phase (Station Requests Repeat)

| State | Description | Data |
|-------|-------------|------|
| `CallerRequestingAgn` | Station is sending "AGN" or "?" | `caller: ActiveCaller` |
| `WaitingForUserExchangeRepeat` | Waiting for user to resend exchange (F2) | `caller: ActiveCaller` |

### QSO Complete Phase

| State | Description | Data |
|-------|-------------|------|
| `QsoComplete` | QSO logged, TU being sent | `result: QsoResult` |
| `WaitingForTailEnder` | 100ms pause before potential tail-ender calls | `callers`, `wait_until: Instant` |

## State Transitions

### Main Flow (Happy Path)

```
Idle
  │
  ├─[F1 or Enter (empty)]─► CallingCq
  │                              │
  │                              ▼ [UserMessageComplete]
  │                         WaitingForCallers
  │                              │
  │                              ▼ [300ms elapsed, callers respond]
  │                         StationsCalling
  │                              │
  │                              ├─[Enter (correct call)]─► SendingExchange
  │                              │                              │
  │                              │                              ▼ [UserMessageComplete]
  │                              │                         WaitingToSendExchange
  │                              │                              │
  │                              │                              ▼ [250ms elapsed]
  │                              │                         ReceivingExchange
  │                              │                              │
  │                              │                              ▼ [Enter]
  │                              │                         QsoComplete
  │                              │                              │
  │                              │                              ▼ [UserMessageComplete: TU]
  │                              │                         ┌────┴────┐
  │                              │                         │         │
  │                              │                    [no tail]  [tail-ender]
  │                              │                         │         │
  │                              │                         ▼         ▼
  │                              │                       Idle   WaitingForTailEnder
  │                              │                                   │
  │                              │                                   ▼ [100ms]
  │                              │                              StationsCalling
  │                              │
  │                              └─[F1]─► (restart CQ, callers may retry)
  │
  └─────────────────────────────────────────────────────────────────────────────
```

### Partial Query Flow (F5)

```
StationsCalling
  │
  ├─[F5 (partial in field)]─► QueryingPartial
  │                               │
  │                               ▼ [UserMessageComplete]
  │                          WaitingForPartialResponse
  │                               │
  │                               ├─[match found]─► StationsCalling (filtered)
  │                               │
  │                               └─[no match]─► WaitingForCallers
```

### Callsign AGN Flow (F8 in callsign field)

```
StationsCalling
  │
  └─[F8]─► SendingCallsignAgn
               │
               ▼ [UserMessageComplete]
          WaitingForCallsignAgn
               │
               ▼ [250ms elapsed]
          StationsCalling (same callers repeat)
```

### Exchange AGN Flow (F8 in exchange field)

```
ReceivingExchange
  │
  └─[F8]─► SendingAgn
               │
               ▼ [UserMessageComplete]
          WaitingForAgn
               │
               ▼ [250ms elapsed]
          ReceivingExchange (same caller resends exchange)
```

### Call Correction Flow (User entered wrong callsign)

```
StationsCalling
  │
  └─[Enter (wrong call)]─► SendingExchangeWillCorrect
                               │
                               ▼ [UserMessageComplete]
                          WaitingToSendCallCorrection
                               │
                               ▼ [250ms elapsed]
                          SendingCallCorrection
                               │
                               ▼
                          WaitingForCallCorrection
                               │
                               ├─[Enter (correct)]─► SendingExchange
                               │
                               ├─[Enter (still wrong, attempts < max)]─► (repeat correction)
                               │
                               ├─[Enter (still wrong, attempts >= max)]─► SendingExchange
                               │
                               ├─[F8]─► SendingCallsignAgnFromCorrection ─► ... ─► WaitingForCallCorrection
                               │
                               └─[F5]─► QueryingPartialFromCorrection ─► ... ─► WaitingForCallCorrection
```

Note: Call correction only triggers ~80% of the time when callsign is wrong. Otherwise proceeds directly to `SendingExchange`.

### F8/F5 During Call Correction Flow

When the user is in `WaitingForCallCorrection` and presses F8 or F5, they can request the station repeat their callsign:

```
WaitingForCallCorrection
  │
  ├─[F8]─► SendingCallsignAgnFromCorrection
  │             │
  │             ▼ [UserMessageComplete]
  │        WaitingForCallsignAgnFromCorrection
  │             │
  │             ▼ [250ms elapsed]
  │        SendingCorrectionRepeat (station sends callsign)
  │             │
  │             ▼ [StationComplete]
  │        WaitingForCallCorrection
  │
  └─[F5]─► QueryingPartialFromCorrection
                │
                ▼ [UserMessageComplete]
           WaitingForPartialResponseFromCorrection
                │
                ▼ [250ms elapsed]
           SendingCorrectionRepeat (station sends callsign)
                │
                ▼ [StationComplete]
           WaitingForCallCorrection
```

### Caller Requests AGN Flow

```
WaitingToSendExchange
  │
  └─[~10% chance]─► CallerRequestingAgn
                        │
                        ▼ [StationComplete]
                   WaitingForUserExchangeRepeat
                        │
                        └─[F2]─► SendingExchange
```

### CQ Restart (Persistent Callers)

When user presses F1 during `StationsCalling`:
1. `CallerManager.on_cq_restart()` is called
2. Active callers have their attempt count incremented
3. Callers with remaining patience set a retry delay
4. Callers who exceeded patience are marked `GaveUp`
5. After CQ completes, surviving callers may respond again

```
StationsCalling
  │
  └─[F1]─► CallingCq
               │
               ▼ [UserMessageComplete]
          WaitingForCallers
               │
               ▼ [300ms, some previous callers + new callers respond]
          StationsCalling
```

## Key Bindings

| Key | Context | Action |
|-----|---------|--------|
| F1 | Any | Stop all, send CQ (callers may retry) |
| Enter | Callsign field (empty) | Same as F1 |
| Enter | Callsign field (text) | Submit callsign, send exchange |
| Enter | Exchange field | Submit exchange, log QSO |
| F2 | WaitingForUserExchangeRepeat | Resend exchange |
| F2 | StationsCalling | Send exchange to matching caller |
| F3 | Any | Send TU |
| F5 | StationsCalling | Query partial callsign |
| F8 | Callsign field | Request callsign repeat |
| F8 | Exchange field | Request exchange repeat |
| F12 | Any | Wipe (clear both fields) |
| Tab | Any | Switch between callsign/exchange fields |
| Escape | Any | Clear fields, stop audio |
| Up/Down | Any | Adjust user WPM |

## Caller Manager

The `CallerManager` maintains a persistent queue of callers:

### PersistentCaller Properties
- `params: StationParams` - Callsign, exchange, WPM, frequency offset, amplitude
- `patience: u8` - How many attempts before giving up (2-5)
- `attempts: u8` - Current attempt count
- `state: CallerState` - Waiting, Calling, GaveUp, Worked
- `reaction_delay_ms: u32` - How fast this caller responds (100-800ms)
- `ready_at: Instant` - When caller is ready to try again

### CallerState
- `Waiting` - In queue, ready to call
- `Calling` - Currently transmitting
- `GaveUp` - Exceeded patience, left frequency
- `Worked` - Successfully completed QSO

### Key Methods
- `on_cq_complete()` - Select callers to respond to CQ
- `on_cq_restart()` - Handle F1 during active callers (increment attempts, set retry delay)
- `on_qso_complete(id)` - Mark caller as worked
- `try_spawn_tail_ender()` - Attempt to spawn a tail-ender after QSO

### Caller Selection and Retry Probability

When `on_cq_complete()` selects which callers respond, two factors determine if a caller participates:

1. **Retry Delay**: After `on_cq_restart()`, each caller gets a random delay (200-1200ms) before they're ready again. Callers check `ready_at` to see if they can call.

2. **Call Probability**: Even when ready, callers have a probability-based chance to "sit out":
   ```
   call_probability = 0.5 + (patience - 1) * 0.1
   ```
   - Patience 2: 60% chance to call
   - Patience 3: 70% chance to call
   - Patience 5: 90% chance to call

This means callers don't always call back-to-back. A caller with patience 3 might call on rounds 1 and 3, skipping round 2.

### Call Correction Probability

When a user submits an incorrect callsign, correction behavior is probabilistic:

1. **Correction Decision**: 80% chance the station corrects, 20% just proceeds (busted call)

2. **Correction Format**: Station sends their callsign once (75%) or twice for emphasis (25%)

3. **Max Attempts**: Station will try to correct up to 2 times before giving up

## Configuration

### SimulationSettings

```toml
[simulation]
max_simultaneous_stations = 2
station_probability = 0.7
wpm_min = 28
wpm_max = 36
frequency_spread_hz = 400.0
amplitude_min = 0.4
amplitude_max = 1.0
agn_request_probability = 0.1
same_country_filter_enabled = false
same_country_probability = 0.1
```

### PileupSettings

```toml
[simulation.pileup]
min_patience = 2
max_patience = 5
retry_delay_min_ms = 200
retry_delay_max_ms = 1200
```

### CallCorrectionSettings

```toml
[simulation.call_correction]
correction_probability = 0.8
max_correction_attempts = 2
```

## Timing Constants

| Delay | Duration | Purpose |
|-------|----------|---------|
| Post-CQ delay | 300ms | Before callers start responding |
| Post-exchange delay | 250ms | Before station sends their exchange |
| Post-AGN delay | 250ms | Before station repeats |
| Post-partial delay | 250ms | Before station repeats callsign |
| Tail-ender delay | 100ms | Before tail-ender starts calling |
| Caller retry delay | 200-1200ms | Before persistent caller tries again |
| Caller reaction time | 100-800ms | How fast a caller responds to CQ |

## Audio Commands

| Command | Description |
|---------|-------------|
| `PlayUserMessage { message, wpm, radio_index }` | Play user's CW message on specified radio |
| `StartStation(StationParams)` | Start a station sending CW |
| `StopStation(id)` | Stop a specific station |
| `StopAll` | Stop all audio (except noise) |
| `UpdateSettings(AudioSettings)` | Update audio configuration |
| `UpdateStereoMode { stereo_enabled, focused_radio }` | Update 2BSIQ stereo routing |
| `Update2BsiqMode { enabled }` | Enable/disable 2BSIQ mode (controls sidetone) |
| `UpdateLatchMode { enabled }` | Enable/disable latch mode |

## Audio Events

| Event | Description |
|-------|-------------|
| `UserMessageComplete` | User's transmission finished |
| `StationComplete { id, radio_index }` | Station finished transmitting (includes radio for routing) |

## 2BSIQ (Two Radio) Mode

2BSIQ mode enables simultaneous operation of two radios, with Radio 1 in the left ear and Radio 2 in the right ear.

### Architecture

In 2BSIQ mode, the application maintains parallel state for each radio:

```rust
pub struct RadioState {
    pub state: ContestState,
    pub callsign_input: String,
    pub exchange_input: String,
    pub current_field: InputField,
    pub last_qso_result: Option<QsoResult>,
    pub last_cq_finished: Option<Instant>,
    pub used_agn_callsign: bool,
    pub used_agn_exchange: bool,
}
```

The main app fields (`self.state`, `self.callsign_input`, etc.) represent the **focused radio**. A sync mechanism copies state to/from the appropriate `RadioState` when focus changes:

- `sync_to_radio_state()`: Copies main fields → focused radio's RadioState
- `sync_from_radio_state()`: Copies focused radio's RadioState → main fields

### Caller Management

Each radio has its own `CallerManager`:
- `caller_manager`: Radio 1 (left channel, `radio_index=0`)
- `caller_manager2`: Radio 2 (right channel, `radio_index=1`)

Stations are spawned with the appropriate `radio_index` in their `StationParams`, which routes their audio to the correct channel.

### Audio Event Routing

`StationComplete` events include `radio_index` to route to the correct radio's state:

```rust
AudioEvent::StationComplete { id, radio_index } => {
    // Route to correct caller manager
    if radio_index == 1 {
        self.caller_manager2.station_audio_complete(id);
    } else {
        self.caller_manager.station_audio_complete(id);
    }
    
    // Update correct radio's state
    let state = if radio_index == focused_radio {
        &mut self.state  // Focused radio uses main state
    } else {
        &mut self.radio1.state  // or radio2.state
    };
    // ... state transitions
}
```

### Audio Routing Modes

| Mode | Behavior |
|------|----------|
| **Stereo ON** | Radio 1 → Left ear, Radio 2 → Right ear |
| **Stereo OFF** | Focused radio → Both ears |
| **Latch Mode** | During TX, other radio → Both ears |

### TX Behavior

- **No sidetone**: User does not hear their own transmission in 2BSIQ mode
- **Visual TX indicator**: Shows transmission progress character-by-character
- **Single TX**: Only one radio can transmit at a time (current limitation)

### 2BSIQ Key Bindings

| Key | Action |
|-----|--------|
| Insert | Swap radio focus |
| ` (backtick) | Toggle stereo mode |
| Ctrl+Left | Focus Radio 1 |
| Ctrl+Right | Focus Radio 2 |
| Ctrl+F1 | Send CQ on alternate radio |
| Ctrl+F3 | Send TU on alternate radio |

Standard F-keys (F1, F2, F3, etc.) operate on the focused radio.
