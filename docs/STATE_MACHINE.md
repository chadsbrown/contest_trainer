# Contest Trainer State Machine

This document describes the state machine that governs the contest simulation flow. It serves as both a developer reference and context for understanding the application behavior.

## Overview

The contest trainer simulates a CW (Morse code) contest QSO flow. The user calls CQ, stations respond, and the user exchanges information with them. The state machine manages all the transitions between these phases.

## Architecture

### Design Philosophy: Information-Driven State Machine

The state machine uses an **information-driven** design rather than an action-driven one. Instead of having many states that encode every possible action combination (which led to 27+ states), we have:

1. **Minimal states** (~9 states) that describe *who is transmitting/waiting*
2. **Rich context** (`QsoContext`) that tracks progress and situation details
3. **Progress tracking** (`QsoProgress`) that records what information has been exchanged

This design enables:
- Flexible user actions (F2/F5/F8 available in more contexts)
- Graceful recovery from TX interruptions
- Clean architecture for future 2BSIQ mode (two independent state machines)

### Key Components

- **ContestState**: Minimal enum (~9 states) describing who is transmitting
- **QsoContext**: Holds all QSO state data (callers, correction status, timers)
- **QsoProgress**: Tracks what information has been sent/received
- **CallerResponse**: Determines how a caller responds based on what they've heard
- **CallerManager**: Manages a persistent queue of callers with patience/retry behavior
- **ActiveCaller**: Wrapper around StationParams for a station currently in play
- **InputField**: Tracks whether user is in Callsign or Exchange field

### Audio Events

Three types of audio events drive state transitions:
- **UserMessageComplete**: User's transmission finished (CQ, exchange, TU, AGN)
- **UserSegmentComplete(type)**: A segment of user's message finished (updates QsoProgress)
- **StationComplete(id)**: A station finished transmitting

## Data Structures

### QsoProgress - Tracks Communication Status

```rust
pub struct QsoProgress {
    /// We have completed sending the caller's callsign
    pub sent_their_call: bool,
    /// We have completed sending our exchange
    pub sent_our_exchange: bool,
    /// We have received the caller's callsign (user entered something)
    pub received_their_call: bool,
    /// We have received the caller's exchange (user entered something)
    pub received_their_exchange: bool,
}
```

The `QsoProgress` struct is updated by `UserSegmentComplete` events from the audio engine, which fire when each segment of a user message finishes playing. This enables accurate tracking even if a transmission is interrupted.

### QsoContext - Holds All QSO State Data

```rust
pub struct QsoContext {
    pub progress: QsoProgress,
    pub current_caller: Option<ActiveCaller>,
    pub active_callers: Vec<ActiveCaller>,
    pub correction_in_progress: bool,
    pub correction_attempts: u8,
    pub wait_until: Option<Instant>,
    pub expecting_callsign_repeat: bool,
}
```

Key fields:
- `current_caller`: The single caller we're currently working with
- `active_callers`: All callers responding (for pileup situations)
- `correction_in_progress`: Whether the station is correcting the user's callsign copy
- `expecting_callsign_repeat`: Set after F5/F8 to tell `handle_station_response()` to have caller repeat their callsign

### ContestState - Minimal State Enum

```rust
pub enum ContestState {
    Idle,
    CallingCq,
    WaitingForCallers,
    StationsCalling,
    UserTransmitting { tx_type: UserTxType },
    WaitingForStation,
    StationTransmitting { tx_type: StationTxType },
    QsoComplete,
    WaitingForTailEnder,
}
```

### UserTxType - What User is Sending

```rust
pub enum UserTxType {
    Cq,           // CQ call
    Exchange,     // Their call + our exchange (Enter in callsign field)
    CallsignOnly, // Just their callsign (F5)
    ExchangeOnly, // Just our exchange (F2)
    Agn,          // AGN/? request (F8)
    Tu,           // TU (F3 or after logging)
}
```

### StationTxType - What Station is Sending

```rust
pub enum StationTxType {
    CallingUs,      // Station(s) sending their callsign
    SendingExchange,// Station sending their exchange
    RequestingAgn,  // Station sending "AGN" or "?"
    Correction,     // Station correcting user's callsign copy
}
```

## Caller Response Logic

The `CallerResponse` enum determines how a caller responds based on `QsoProgress`:

| sent_their_call | sent_our_exchange | CallerResponse |
|-----------------|-------------------|----------------|
| false | false | Confused (resends call or "?") |
| false | true | Confused (unusual case) |
| true | false | RequestAgn (sends "AGN" or "?") |
| true | true | SendExchange (sends their exchange) |

This is implemented in `CallerResponse::from_progress()`.

**Special cases in `handle_station_response()`:**

1. **`expecting_callsign_repeat = true`**: Caller repeats their callsign (after F5 or F8 in callsign field)
2. **`correction_in_progress = true`**: Caller sends correction (75% once, 25% twice for emphasis)
3. **Otherwise**: Uses `CallerResponse::from_progress()` to determine response

## State Transitions

### States Overview

| State | Description |
|-------|-------------|
| `Idle` | Waiting for user to start |
| `CallingCq` | User is sending CQ message |
| `WaitingForCallers` | CQ finished, 300ms delay before stations respond |
| `StationsCalling` | One or more stations have sent/are sending callsigns |
| `UserTransmitting { tx_type }` | User is transmitting (type specifies what) |
| `WaitingForStation` | Brief pause (250ms) before station responds |
| `StationTransmitting { tx_type }` | Station is transmitting (type specifies what) |
| `QsoComplete` | QSO logged, TU being sent |
| `WaitingForTailEnder` | 100ms pause before potential tail-ender calls |

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
  │                              ▼ [Enter (callsign entered)]
  │                         UserTransmitting { Exchange }
  │                              │
  │                              ├─[UserSegmentComplete(TheirCallsign)]
  │                              │   └─► progress.sent_their_call = true
  │                              │
  │                              ├─[UserSegmentComplete(OurExchange)]
  │                              │   └─► progress.sent_our_exchange = true
  │                              │
  │                              ▼ [UserMessageComplete]
  │                         WaitingForStation
  │                              │
  │                              ▼ [250ms elapsed, CallerResponse::SendExchange]
  │                         StationTransmitting { SendingExchange }
  │                              │
  │                              ▼ [Enter in exchange field]
  │                         QsoComplete
  │                              │
  │                              ▼ [UserMessageComplete: TU]
  │                         ┌────┴────┐
  │                         │         │
  │                    [no tail]  [tail-ender]
  │                         │         │
  │                         ▼         ▼
  │                       Idle   WaitingForTailEnder
  │                                   │
  │                                   ▼ [100ms]
  │                              StationsCalling
```

### F5 - Send His Call (Callsign Only)

F5 always sends only the callsign field contents. It works in any state with active callers.

```
Any state with active callers
  │
  └─[F5]─► (StopAll audio)
           (Select matching caller if found)
           (Set expecting_callsign_repeat = true)
               │
               ▼
           UserTransmitting { CallsignOnly }
               │
               ▼ [UserMessageComplete]
           WaitingForStation
               │
               ▼ [250ms, expecting_callsign_repeat = true]
           StationsCalling (caller repeats their callsign)
```

**Multiple AGN requests for callsign (e.g., user presses F5 three times):**

```
StationsCalling
  │
  └─[F5]─► UserTransmitting { CallsignOnly }
               ▼
           WaitingForStation
               ▼
           StationsCalling (caller repeats)
               │
               └─[F5]─► UserTransmitting { CallsignOnly }
                            ▼
                        WaitingForStation
                            ▼
                        StationsCalling (caller repeats again)
                            │
                            └─[F5]─► ... (can repeat indefinitely)
```

### F8 - Request AGN

F8 behavior depends on which field has focus:

**In callsign field (`StationsCalling` state):**
```
StationsCalling
  │
  └─[F8]─► (StopAll audio)
           (Set expecting_callsign_repeat = true)
               │
               ▼
           UserTransmitting { Agn }
               │
               ▼ [UserMessageComplete]
           WaitingForStation
               │
               ▼ [250ms, expecting_callsign_repeat = true]
           StationsCalling (caller repeats their callsign)
```

**In exchange field (`StationTransmitting { SendingExchange }` state):**
```
StationTransmitting { SendingExchange }
  │
  └─[F8]─► (StopAll audio)
               │
               ▼
           UserTransmitting { Agn }
               │
               ▼ [UserMessageComplete]
           WaitingForStation
               │
               ▼ [250ms, CallerResponse::SendExchange]
           StationTransmitting { SendingExchange } (caller resends exchange)
```

**Multiple AGN requests for exchange:**

```
StationTransmitting { SendingExchange }
  │
  └─[F8]─► UserTransmitting { Agn }
               ▼
           WaitingForStation
               ▼
           StationTransmitting { SendingExchange } (resends)
               │
               └─[F8]─► UserTransmitting { Agn }
                            ▼
                        WaitingForStation
                            ▼
                        StationTransmitting { SendingExchange } (resends again)
                            │
                            └─[F8]─► ... (can repeat indefinitely)
```

### F2 - Send Exchange Only

F2 always sends only the exchange. It works in any state with active callers.

```
Any state with active callers
  │
  └─[F2]─► (StopAll audio)
           (Select matching caller if found)
               │
               ▼
           UserTransmitting { ExchangeOnly }
               │
               ├─[UserSegmentComplete(OurExchange)]
               │   └─► progress.sent_our_exchange = true
               │
               ▼ [UserMessageComplete]
           WaitingForStation
               │
               ▼ [250ms, based on CallerResponse]
           (next state depends on progress)
```

### Call Correction Flow

When user enters wrong callsign:

```
StationsCalling
  │
  └─[Enter (wrong call)]─► 
           (80% chance: correction_in_progress = true)
           (20% chance: no correction, busted call)
               │
               ▼
           UserTransmitting { Exchange }
               │
               ▼ [UserMessageComplete]
           WaitingForStation
               │
               ▼ [250ms, correction_in_progress = true]
           StationTransmitting { Correction }
               │  (Station sends callsign once (75%) or twice (25%))
               │
               ▼ [StationComplete]
           StationsCalling
               │  (Status: "Fix callsign and press Enter")
               │
               ├─[Enter (correct)]─► correction ends, normal flow
               │
               ├─[Enter (still wrong, attempts < max)]─► repeat correction
               │
               ├─[Enter (attempts >= max)]─► correction ends, busted call
               │
               ├─[F5]─► Send callsign, expecting_callsign_repeat = true
               │
               └─[F8]─► Request repeat, expecting_callsign_repeat = true
```

### Caller Requests AGN Flow

When the caller (station) needs our exchange repeated:

```
WaitingForStation
  │
  └─[~10% chance per CallerResponse::SendExchange]
           │
           ▼
       StationTransmitting { RequestingAgn }
           │  (Station sends "AGN" or "?")
           │
           ▼ [StationComplete]
       StationsCalling
           │  (Status: "Station requests repeat - press F2")
           │
           └─[F2]─► UserTransmitting { ExchangeOnly }
                        │
                        ▼
                    WaitingForStation
                        │
                        ▼ [CallerResponse::SendExchange]
                    StationTransmitting { SendingExchange }
```

### CQ Restart (Persistent Callers)

When user presses F1 during active QSO:

```
StationsCalling (or any state with callers)
  │
  └─[F1]─► (StopAll audio)
           (CallerManager.on_cq_restart())
           (context.reset())
               │
               ▼
           CallingCq
               │
               ▼ [UserMessageComplete]
           WaitingForCallers
               │
               ▼ [300ms, some previous callers + new callers respond]
           StationsCalling
```

The `CallerManager` handles persistence:
1. Active callers have their attempt count incremented
2. Callers with remaining patience get a retry delay (200-1200ms)
3. Callers who exceeded patience are marked `GaveUp`
4. After CQ completes, surviving callers may respond again

## Key Bindings

| Key | Context | Action |
|-----|---------|--------|
| F1 | Any | Stop all, send CQ (callers may retry) |
| Enter | Callsign field (empty) | Same as F1 |
| Enter | Callsign field (text) | Submit callsign, send call + exchange |
| Enter | Exchange field | Submit exchange, log QSO |
| F2 | Any (with active caller) | Send exchange only |
| F3 | Any | Send TU |
| F5 | Any (with active caller) | Send his call (callsign only) |
| F8 | Callsign field | Request callsign repeat |
| F8 | Exchange field | Request exchange repeat |
| F12 | Any | Wipe (clear both fields) |
| Tab | Any | Switch between callsign/exchange fields |
| Escape | Any | Stop transmission (does not clear fields) |
| Up/Down | Any | Adjust user WPM |

**Note on F2/F5:** These now work in any state with an active caller, stopping current audio if needed. This allows recovery from mistakes (e.g., typo the callsign, press Escape, press F5 to resend just the call, then F2 to resend just the exchange).

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

1. **Retry Delay**: After `on_cq_restart()`, each caller gets a random delay (200-1200ms) before they're ready again.

2. **Call Probability**: Even when ready, callers have a probability-based chance to "sit out":
   ```
   call_probability = 0.5 + (patience - 1) * 0.1
   ```
   - Patience 2: 60% chance to call
   - Patience 3: 70% chance to call
   - Patience 5: 90% chance to call

### Call Correction Probability

When a user submits an incorrect callsign:

1. **Correction Decision**: 80% chance the station corrects, 20% just proceeds (busted call)
2. **Correction Format**: Station sends their callsign once (75%) or twice (25%)
3. **Max Attempts**: Station will try to correct up to 2 times before giving up

## Segmented Audio Messages

User messages that contain multiple logical parts (callsign + exchange) are sent as segmented messages:

```rust
let segments = vec![
    MessageSegment {
        content: their_call.to_string(),
        segment_type: MessageSegmentType::TheirCallsign,
    },
    MessageSegment {
        content: exchange,
        segment_type: MessageSegmentType::OurExchange,
    },
];
```

The audio engine tracks segment boundaries and emits `UserSegmentComplete` events as each segment finishes. This allows accurate `QsoProgress` updates even if the transmission is interrupted (e.g., by Escape or F1).

### MessageSegmentType
- `TheirCallsign` - The caller's callsign
- `OurExchange` - Our exchange info
- `Cq` - CQ message
- `Tu` - Thank you
- `Agn` - AGN request

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
| Post-user-TX delay | 250ms | Before station responds to user |
| Tail-ender delay | 100ms | Before tail-ender starts calling |
| Caller retry delay | 200-1200ms | Before persistent caller tries again |
| Caller reaction time | 100-800ms | How fast a caller responds to CQ |

## Audio Commands

| Command | Description |
|---------|-------------|
| `PlayUserMessage { message, wpm }` | Play user's CW message (plain) |
| `PlayUserMessageSegmented { segments, wpm }` | Play segmented message with progress tracking |
| `StartStation(StationParams)` | Start a station sending CW |
| `StopAll` | Stop all audio (except noise) |
| `UpdateSettings(AudioSettings)` | Update audio configuration |

## Audio Events

| Event | Description |
|-------|-------------|
| `UserMessageComplete` | User's transmission finished |
| `UserSegmentComplete(type)` | A segment of user message finished |
| `StationComplete(id)` | Station finished transmitting |

## Future: 2BSIQ Readiness

This design prepares for 2BSIQ (two radios) by:

1. **Per-radio state**: Each radio gets its own `ContestState` + `QsoContext`
2. **Independent progress**: Each radio has its own `QsoProgress`
3. **Shared constraint**: Only one radio can be in `UserTransmitting` at a time
4. **Clean interruption**: Starting TX on radio B can interrupt radio A

Future 2BSIQ structure:
```rust
struct TwoBsiqApp {
    radio_a: RadioState,  // ContestState + QsoContext
    radio_b: RadioState,
    current_tx: Option<Radio>,
}
```
