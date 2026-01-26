# Plan: 2BSIQ Mode Implementation (v2)

## Overview

Add a 2BSIQ (Two Radio) mode where the user operates two completely independent radios simultaneously:
- **Radio 1**: Left ear (left audio channel)
- **Radio 2**: Right ear (right audio channel)
- Each radio has its own state machine, caller pool, noise, and audio
- User switches focus between radios to work independent QSOs
- **Key constraint**: Only one radio can transmit at a time

## Core Design Principles

1. **Complete Independence**: Each radio is a fully separate simulation. The only connection is the shared constraint that the user cannot transmit on both simultaneously.

2. **TX Interruption**: If the user triggers TX on one radio while the other is transmitting, the first TX is immediately interrupted. The interrupted radio's state machine remains in place, and the caller will likely send AGN/?.

3. **Dual Mixer Architecture**: Two separate `Mixer` instances, one per radio, combined at the audio output stage. This ensures true independence.

4. **Visual TX Feedback**: Since there's no sidetone in 2BSIQ mode, a TX progress indicator shows what's being transmitted character-by-character.

---

## Keyboard Controls

### Radio Focus Keys (Configurable)

| Key | Action | Notes |
|-----|--------|-------|
| **Insert** | Swap radio focus | Configurable in settings |
| **Backtick (`)** | Toggle stereo/mono | Configurable; never echoes to input fields |
| **Ctrl+Left** | Focus Radio 1 | Fixed |
| **Ctrl+Right** | Focus Radio 2 | Fixed |

### Function Keys

| Key | Action |
|-----|--------|
| **F1-F8, Enter, Tab, etc.** | Apply to focused radio only |
| **Ctrl+F1 to Ctrl+F8** | Send Fn message on non-focused radio |

### Typing

All character input goes to the focused radio's active field. Backtick must be intercepted before character processing.

---

## Audio Behavior

### Channel Routing

| Mode | Radio 1 | Radio 2 |
|------|---------|---------|
| **Stereo** | Left ear only | Right ear only |
| **Mono** | Both ears (if focused) | Both ears (if focused) |
| **Latch** (during TX) | If R1 TXing: muted | If R1 TXing: both ears |

### Stereo Mode (Default)
- Radio 1 audio → left channel only
- Radio 2 audio → right channel only
- True separation for 2BSIQ practice

### Mono Mode
- Focused radio's audio → both channels
- Useful for concentrating on one radio temporarily
- When focus changes, audio routing changes with it

### Latch Mode (Optional, Toggle in Settings)
- When enabled and one radio is transmitting:
  - Transmitting radio is muted (no sidetone anyway)
  - Non-transmitting radio routes to BOTH ears
- When TX ends, returns to stereo/mono based on current setting
- Purpose: Full attention on receiving radio while sending

### No Sidetone
- User's transmissions produce no audio in 2BSIQ mode
- TX progress indicator provides visual feedback instead

### Independent Noise
- Each radio has its own `NoiseGenerator` instance
- Noise is NOT synchronized between radios (different random seeds)
- Crashes, pops, QRN are independent per radio

### Per-Radio Volume
- Each radio has a volume slider (0.0-1.0)
- Applied before stereo/mono/latch routing

---

## TX Interruption Handling

### When TX is Interrupted

**Scenario**: Radio 1 is sending exchange. User switches to Radio 2 and presses F1 (CQ).

1. Radio 1's audio TX stops immediately
2. Radio 2's CQ starts
3. Radio 1's state machine remains in current state (e.g., `SendingExchange`)
4. Radio 1's TX indicator text persists (shows what was being sent)
5. Radio 1's caller received partial/garbled message
6. When Radio 2's TX completes:
   - User can switch back to Radio 1
   - Caller on Radio 1 sends AGN or ? (confused by interruption)
   - User can retry the transmission (F2 for exchange, etc.)

### State Machine Changes for Interruption

Each radio's state machine needs to handle:
- `tx_interrupted: bool` flag (or similar)
- When TX is interrupted, don't transition to next state
- Caller behavior: after interruption, caller sends AGN/? instead of normal response
- User can retry the same action to continue

### TX Indicator Persistence

- TX indicator text remains visible until the user takes another TX action on that radio
- This helps user remember where they were in the QSO flow after handling the other radio
- When a new TX starts, the indicator updates to the new message

---

## UI Layout

### Main Window (2BSIQ Mode)

```
┌─────────────────────────────────────────────────────────────────────┐
│ Contest: CQ WW                                    [2BSIQ Mode]      │
│ QSOs: 24  Points: 240  Rate: 48/hr  WPM: 32                        │
├────────────────────────────────┬────────────────────────────────────┤
│ ► RADIO 1 (LEFT)               │   RADIO 2 (RIGHT)                  │
│ ┌────────────────────────────┐ │ ┌────────────────────────────────┐ │
│ │ Status: Receiving exchange │ │ │ Status: Idle                   │ │
│ │ Call: [W1ABC___________]   │ │ │ Call: [_______________]        │ │
│ │ Exch: [5NN 05_____]        │ │ │ Exch: [_______________]        │ │
│ │ TX: W1ABC 5NN 05           │ │ │ TX:                            │ │
│ │ Last: K4XYZ OK/OK +3       │ │ │ Last: (none)                   │ │
│ │ Vol: [████████──] 0.8      │ │ │ Vol: [██████████] 1.0          │ │
│ └────────────────────────────┘ │ └────────────────────────────────┘ │
├────────────────────────────────┴────────────────────────────────────┤
│ F1:CQ F2:Exch F3:TU F5:His F8:AGN | Ins:Swap `:Stereo Ctrl+Fn:Other │
├─────────────────────────────────────────────────────────────────────┤
│ [STEREO] [LATCH: OFF]                              Focus: Radio 1   │
└─────────────────────────────────────────────────────────────────────┘
```

### Panel Components

Each radio panel contains:
1. **Header**: "RADIO 1 (LEFT)" or "RADIO 2 (RIGHT)"
2. **Status line**: Current state description
3. **Call field**: Callsign input (editable when focused, readonly when not)
4. **Exch field**: Exchange input (editable when focused, readonly when not)
5. **TX indicator**: Shows last/current TX message, persists until next TX
6. **Last QSO**: Result of last QSO on this radio
7. **Volume slider**: Per-radio volume control

### Focus Indication

The focused radio panel has:
- Arrow/chevron (►) before the radio name
- Highlighted border (thicker or colored)
- Subtle background tint
- Input fields are editable

The non-focused radio panel has:
- No arrow
- Normal border
- Normal background
- Input fields are readonly (displayed but not editable)

### Status Bar

Bottom status bar shows:
- `[STEREO]` or `[MONO]` - current audio mode
- `[LATCH: ON]` or `[LATCH: OFF]` - latch mode status
- `Focus: Radio 1` or `Focus: Radio 2` - current focus

### Key Hints

Show context-appropriate key hints:
- Standard function keys (F1, F2, F3, F5, F8)
- 2BSIQ-specific keys (Insert, Backtick, Ctrl+Fn)

---

## Data Structures

### RadioId Enum

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadioId {
    Radio1,
    Radio2,
}

impl RadioId {
    pub fn other(&self) -> RadioId {
        match self {
            RadioId::Radio1 => RadioId::Radio2,
            RadioId::Radio2 => RadioId::Radio1,
        }
    }
    
    pub fn index(&self) -> usize {
        match self {
            RadioId::Radio1 => 0,
            RadioId::Radio2 => 1,
        }
    }
}
```

### RadioState Struct

```rust
pub struct RadioState {
    // State machine
    pub state: ContestState,
    
    // Input fields
    pub callsign_input: String,
    pub exchange_input: String,
    pub current_field: InputField,
    
    // QSO tracking
    pub last_qso_result: Option<QsoResult>,
    pub used_agn_callsign: bool,
    pub used_agn_exchange: bool,
    
    // TX tracking
    pub last_tx_message: Option<String>,  // Persists until next TX
    pub tx_was_interrupted: bool,         // For caller AGN behavior
    
    // Timing
    pub last_cq_finished: Option<Instant>,
    
    // Caller management (owned by this radio)
    pub caller_manager: CallerManager,
}
```

### ContestApp Changes (2BSIQ Mode)

```rust
pub struct ContestApp {
    // Mode flag
    pub two_bsiq_enabled: bool,
    
    // Single-radio state (used when 2BSIQ disabled)
    pub state: ContestState,
    pub callsign_input: String,
    pub exchange_input: String,
    pub current_field: InputField,
    pub caller_manager: CallerManager,
    // ... other single-radio fields ...
    
    // 2BSIQ state (used when 2BSIQ enabled)
    pub radio1: RadioState,
    pub radio2: RadioState,
    pub focused_radio: RadioId,
    
    // 2BSIQ audio state
    pub stereo_enabled: bool,      // true = stereo, false = mono
    pub latch_mode: bool,          // hear other radio during TX
    pub transmitting_radio: Option<RadioId>,  // which radio is currently TXing
    
    // 2BSIQ settings (configurable)
    pub radio1_volume: f32,
    pub radio2_volume: f32,
    pub swap_radio_key: egui::Key,     // default: Insert
    pub stereo_toggle_key: egui::Key,  // default: Backtick/Grave
    
    // ... shared fields (settings, stats, etc.) ...
}
```

### Dual Mixer Architecture

```rust
// In audio module
pub struct DualRadioAudio {
    pub mixer1: Mixer,  // Radio 1
    pub mixer2: Mixer,  // Radio 2
}

impl DualRadioAudio {
    pub fn fill_stereo_buffer(
        &mut self,
        buffer: &mut [f32],  // Interleaved L/R samples
        stereo_enabled: bool,
        focused_radio: RadioId,
        latch_mode: bool,
        transmitting_radio: Option<RadioId>,
        radio1_volume: f32,
        radio2_volume: f32,
    ) {
        // Get samples from each mixer
        let samples1 = self.mixer1.fill_buffer(...);
        let samples2 = self.mixer2.fill_buffer(...);
        
        // Route based on mode
        for i in 0..num_frames {
            let left_idx = i * 2;
            let right_idx = i * 2 + 1;
            
            let s1 = samples1[i] * radio1_volume;
            let s2 = samples2[i] * radio2_volume;
            
            if let Some(tx_radio) = transmitting_radio {
                if latch_mode {
                    // Latch: non-TX radio to both ears
                    let other = if tx_radio == RadioId::Radio1 { s2 } else { s1 };
                    buffer[left_idx] = other;
                    buffer[right_idx] = other;
                } else {
                    // Normal: TX radio muted, other radio in its channel
                    match tx_radio {
                        RadioId::Radio1 => {
                            buffer[left_idx] = 0.0;  // R1 muted
                            buffer[right_idx] = s2;
                        }
                        RadioId::Radio2 => {
                            buffer[left_idx] = s1;
                            buffer[right_idx] = 0.0;  // R2 muted
                        }
                    }
                }
            } else if stereo_enabled {
                // Stereo: R1 left, R2 right
                buffer[left_idx] = s1;
                buffer[right_idx] = s2;
            } else {
                // Mono: focused radio to both ears
                let focused = if focused_radio == RadioId::Radio1 { s1 } else { s2 };
                buffer[left_idx] = focused;
                buffer[right_idx] = focused;
            }
        }
    }
}
```

---

## Audio Commands & Events

### Extended AudioCommand

```rust
pub enum AudioCommand {
    // Existing commands (work in single-radio mode)
    PlayUserMessage { message: String, wpm: u32 },
    StartStation(StationParams),
    StopStation(StationId),
    StopAll,
    UpdateSettings(AudioSettings),
    
    // 2BSIQ-specific commands
    PlayUserMessage2BSIQ { message: String, wpm: u32, radio: RadioId },
    StartStation2BSIQ(StationParams, RadioId),
    StopRadio(RadioId),
    
    // Mode updates
    Update2BsiqMode { enabled: bool },
    UpdateStereoMode { stereo: bool, focused: RadioId },
    UpdateLatchMode { enabled: bool },
    UpdateRadioVolumes { radio1: f32, radio2: f32 },
}
```

### Extended AudioEvent

```rust
pub enum AudioEvent {
    // Existing events (single-radio mode)
    UserMessageComplete,
    StationComplete(StationId),
    
    // 2BSIQ events include radio identification
    UserMessageComplete2BSIQ { radio: RadioId },
    StationComplete2BSIQ { id: StationId, radio: RadioId },
    
    // TX interruption
    TxInterrupted { radio: RadioId },
}
```

### TX Progress Tracking

```rust
impl Mixer {
    /// Returns (message, chars_sent) if currently transmitting
    pub fn get_tx_progress(&self) -> Option<(&str, usize)> {
        self.user_station.as_ref().map(|us| {
            (us.message.as_str(), us.chars_sent())
        })
    }
}
```

---

## Configuration

### New Settings Fields

```rust
// In UserSettings or SimulationSettings
pub struct TwoBsiqSettings {
    pub enabled: bool,
    pub latch_mode: bool,
    pub radio1_volume: f32,  // 0.0 - 1.0
    pub radio2_volume: f32,  // 0.0 - 1.0
    pub default_stereo: bool,  // Start in stereo mode
    pub swap_radio_key: String,  // "Insert" by default
    pub stereo_toggle_key: String,  // "Grave" by default
}
```

### Settings File (TOML)

```toml
[two_bsiq]
enabled = false
latch_mode = false
radio1_volume = 1.0
radio2_volume = 1.0
default_stereo = true
swap_radio_key = "Insert"
stereo_toggle_key = "Grave"
```

---

## Statistics Tracking

### Per-Radio Stats

```rust
pub struct RadioStats {
    pub qso_count: u32,
    pub points: u32,
    pub callsign_correct: u32,
    pub callsign_incorrect: u32,
    pub exchange_correct: u32,
    pub exchange_incorrect: u32,
    pub agn_callsign_count: u32,
    pub agn_exchange_count: u32,
}

pub struct SessionStats {
    // Combined stats (always tracked)
    pub total_qsos: u32,
    pub total_points: u32,
    // ... etc ...
    
    // Per-radio stats (only when 2BSIQ enabled)
    pub radio1_stats: Option<RadioStats>,
    pub radio2_stats: Option<RadioStats>,
}
```

### Stats Display

When 2BSIQ mode is active, the stats window shows:
- Combined totals at the top
- Per-radio breakdown below
- Indicates which radio each stat belongs to

---

## Mode Switching

### Enabling 2BSIQ Mode

1. Set `two_bsiq_enabled = true`
2. Initialize `radio1` and `radio2` with fresh `RadioState`
3. Initialize two `CallerManager` instances
4. Set `focused_radio = RadioId::Radio1`
5. Set `stereo_enabled` based on `default_stereo` setting
6. Reset statistics
7. Send `Update2BsiqMode { enabled: true }` to audio engine
8. UI switches to dual-panel layout

### Disabling 2BSIQ Mode

1. Set `two_bsiq_enabled = false`
2. Stop all audio on both radios
3. Reset single-radio state to `Idle`
4. Reset statistics
5. Send `Update2BsiqMode { enabled: false }` to audio engine
6. UI switches to single-panel layout

---

## Implementation Phases

### Phase 1: Dual Mixer Architecture

**Goal**: Create the audio foundation with two independent mixers.

**Changes**:
1. Create `DualRadioAudio` struct with two `Mixer` instances
2. Each mixer has its own `NoiseGenerator`
3. Implement `fill_stereo_buffer()` with stereo routing
4. Update `AudioEngine` to use dual mixers when 2BSIQ enabled
5. Add `Update2BsiqMode` command to switch architectures

**Verification**:
- Test stereo output: different noise in each ear
- Test that single-radio mode still works

**Files**: `src/audio/mixer.rs`, `src/audio/engine.rs`

---

### Phase 2: RadioState and Data Structures

**Goal**: Create the data structures for dual radio state.

**Changes**:
1. Create `RadioId` enum
2. Create `RadioState` struct
3. Add 2BSIQ fields to `ContestApp`
4. Add 2BSIQ settings to config
5. Initialize radio states on startup (when enabled)

**Verification**:
- App compiles and runs
- Single-radio mode still works
- Settings save/load correctly

**Files**: `src/app.rs`, `src/config.rs`

---

### Phase 3: Dual Caller Management

**Goal**: Each radio has its own independent caller pool.

**Changes**:
1. `RadioState` owns its `CallerManager`
2. Each `CallerManager` initialized independently
3. Stations spawned with radio identification
4. Audio events include radio ID for routing

**Verification**:
- Callers appear independently on each radio
- Audio events route to correct radio

**Files**: `src/app.rs`, `src/station/caller_manager.rs`, `src/messages.rs`

---

### Phase 4: Basic Dual Radio UI

**Goal**: Display both radios side-by-side.

**Changes**:
1. Create `render_radio_panel()` function
2. Render two equal-width panels when 2BSIQ enabled
3. Show focus indicator (arrow, border, background)
4. Separate input fields per radio (editable only when focused)
5. Show status, last QSO per radio

**Verification**:
- Both radios visible
- Focus indicator clear
- Fields editable only on focused radio

**Files**: `src/ui/main_panel.rs`

---

### Phase 5: Radio Focus Controls

**Goal**: Implement keyboard controls for radio switching.

**Changes**:
1. Implement Insert key to swap focus (configurable)
2. Implement Ctrl+Left/Right for explicit selection
3. Implement backtick for stereo toggle (configurable, no echo)
4. Route F1-F8, Enter, Tab to focused radio only
5. Update UI to show current focus

**Verification**:
- Insert swaps focus
- Ctrl+arrows select specific radio
- Backtick toggles stereo (no character echo)
- Function keys work on focused radio only

**Files**: `src/app.rs`, `src/ui/main_panel.rs`

---

### Phase 6: Parallel State Machines

**Goal**: Each radio runs its own independent ContestState.

**Changes**:
1. State transitions route to correct radio based on focus
2. Audio events (StationComplete, UserMessageComplete) route by radio ID
3. Each radio can be in different state simultaneously
4. Implement all state handlers for radio-specific operation

**Verification**:
- Work QSO on Radio 1 while Radio 2 is idle
- Complete QSO on Radio 1, Radio 2 unchanged
- Both radios can have active callers

**Files**: `src/app.rs`

---

### Phase 7: TX Coordination

**Goal**: Handle the "only one radio can TX" constraint.

**Changes**:
1. Track `transmitting_radio: Option<RadioId>`
2. When TX triggered on one radio while other is TXing:
   - Stop TX on first radio
   - Set `tx_was_interrupted = true` on first radio
   - Start TX on second radio
3. Caller behavior: if TX was interrupted, caller sends AGN/?
4. Audio routing: mute TX radio, apply latch mode if enabled

**Verification**:
- TX on R1, then trigger TX on R2: R1 stops, R2 starts
- R1's caller sends AGN after interruption
- User can retry on R1 after R2 finishes

**Files**: `src/app.rs`, `src/audio/engine.rs`

---

### Phase 8: TX Progress Indicator

**Goal**: Show transmission progress without sidetone.

**Changes**:
1. Track `last_tx_message` in `RadioState`
2. Expose TX progress (chars sent) from mixer
3. Render TX indicator in each radio panel
4. TX text persists until next TX action on that radio
5. Show cursor/block at current position during TX

**Verification**:
- TX indicator shows character-by-character progress
- Text persists after TX completes
- Text clears when new TX starts

**Files**: `src/audio/mixer.rs`, `src/ui/main_panel.rs`, `src/app.rs`

---

### Phase 9: Stereo/Mono/Latch Audio Modes

**Goal**: Implement all audio routing modes.

**Changes**:
1. Stereo mode: R1 → left, R2 → right
2. Mono mode: focused radio → both ears, updates on focus change
3. Latch mode: during TX, non-TX radio → both ears
4. Per-radio volume controls in UI
5. Status bar indicators for current mode

**Verification**:
- Stereo: distinct audio in each ear
- Mono: focused radio in both ears, changes with focus
- Latch: during TX, other radio fills both ears
- Volume sliders work

**Files**: `src/audio/mixer.rs`, `src/audio/engine.rs`, `src/ui/main_panel.rs`

---

### Phase 10: Ctrl+Fn for Alternate Radio

**Goal**: Send function key messages on non-focused radio.

**Changes**:
1. Detect Ctrl+F1 through Ctrl+F8
2. Route these to the OTHER radio (not focused)
3. Same TX coordination rules apply

**Verification**:
- Focus on R1, Ctrl+F1 sends CQ on R2
- TX coordination works across Ctrl+Fn keys

**Files**: `src/app.rs`

---

### Phase 11: Statistics and Last QSO

**Goal**: Track and display per-radio statistics.

**Changes**:
1. Add `RadioStats` struct
2. Track per-radio stats when 2BSIQ enabled
3. Show per-radio last QSO in each panel
4. Stats window shows 2BSIQ breakdown when in mode
5. Mode switch resets statistics

**Verification**:
- Stats track separately per radio
- Stats window shows breakdown
- Mode switch clears stats

**Files**: `src/stats.rs`, `src/ui/stats_window.rs`, `src/ui/main_panel.rs`

---

### Phase 12: Settings Panel & Persistence

**Goal**: Add 2BSIQ settings to configuration.

**Changes**:
1. Add 2BSIQ section to settings panel
2. Toggle for enabling 2BSIQ mode
3. Latch mode checkbox
4. Key binding configuration (swap radio, stereo toggle)
5. Default stereo/mono selection
6. Persist all settings to config file

**Verification**:
- Settings appear and work
- Changes persist across restarts
- Key bindings are configurable

**Files**: `src/config.rs`, `src/ui/settings_panel.rs`

---

### Phase 13: Status Bar

**Goal**: Add status bar with 2BSIQ indicators.

**Changes**:
1. Create status bar at bottom of window
2. Show stereo/mono indicator
3. Show latch mode status
4. Show current focus
5. Only visible in 2BSIQ mode

**Verification**:
- Status bar shows correct state
- Updates in real-time

**Files**: `src/ui/main_panel.rs`

---

### Phase 14: Polish and Edge Cases

**Goal**: Handle edge cases and polish the experience.

**Changes**:
1. Mode switching mid-session (immediate, resets stats)
2. Contest type switching in 2BSIQ mode
3. Window resizing with dual panels
4. Keyboard focus edge cases
5. Audio glitch prevention during mode/focus changes

**Verification**:
- No crashes on mode toggle
- Clean transitions
- Responsive UI

**Files**: Various

---

### Phase 15: Documentation

**Goal**: Update all documentation.

**Changes**:
1. Update `docs/user-guide.md` with 2BSIQ section
2. Update `docs/STATE_MACHINE.md` with dual radio info
3. Add 2BSIQ keyboard reference

**Verification**:
- Documentation accurate and complete

**Files**: `docs/user-guide.md`, `docs/STATE_MACHINE.md`

---

## Files Summary

| File | Changes |
|------|---------|
| `src/app.rs` | RadioId, RadioState, dual state management, TX coordination |
| `src/audio/mixer.rs` | DualRadioAudio, stereo routing, TX progress |
| `src/audio/engine.rs` | Dual mixer support, 2BSIQ commands |
| `src/messages.rs` | 2BSIQ commands and events with RadioId |
| `src/config.rs` | TwoBsiqSettings |
| `src/ui/main_panel.rs` | Dual panel layout, TX indicator, status bar |
| `src/ui/settings_panel.rs` | 2BSIQ settings section |
| `src/station/caller_manager.rs` | Radio-aware caller spawning |
| `src/stats.rs` | Per-radio statistics |
| `src/ui/stats_window.rs` | 2BSIQ stats display |
| `docs/user-guide.md` | 2BSIQ user documentation |
| `docs/STATE_MACHINE.md` | Dual state machine documentation |

---

## Verification Checklist

### Audio
- [ ] Stereo: Radio 1 in left ear only, Radio 2 in right ear only
- [ ] Mono: Focused radio in both ears
- [ ] Mono focus change: Audio switches when focus changes
- [ ] Latch: During TX, other radio in both ears
- [ ] Independent noise: Different noise in each channel
- [ ] No sidetone: User TX produces no audio
- [ ] Per-radio volume: Sliders work correctly

### TX Coordination
- [ ] Only one radio can TX at a time
- [ ] TX interruption: First TX stops when second starts
- [ ] Caller AGN: Interrupted caller sends AGN/?
- [ ] TX indicator persists until next TX action

### UI
- [ ] Both panels visible and equal width
- [ ] Focus clearly indicated (arrow, border, tint)
- [ ] Input editable only on focused radio
- [ ] TX indicator shows character progress
- [ ] Status bar shows stereo/mono, latch, focus
- [ ] Per-radio volume sliders
- [ ] Per-radio last QSO display

### Keyboard
- [ ] Insert swaps focus
- [ ] Backtick toggles stereo (no echo)
- [ ] Ctrl+Left/Right select radio
- [ ] Ctrl+F1-F8 work on other radio
- [ ] All other keys work on focused radio

### State Machines
- [ ] Radios operate independently
- [ ] Can be in different states simultaneously
- [ ] QSO on R1 doesn't affect R2
- [ ] State transitions route correctly

### Statistics
- [ ] Per-radio tracking
- [ ] Combined totals
- [ ] Stats window shows breakdown in 2BSIQ mode
- [ ] Mode switch resets stats

### Settings
- [ ] 2BSIQ toggle works
- [ ] Latch mode toggle works
- [ ] Key bindings configurable
- [ ] Settings persist

---

## Typical 2BSIQ Workflow Example

1. User starts in 2BSIQ mode, both radios Idle, focus on Radio 1
2. User presses F1 → CQ sent on Radio 1
3. TX indicator shows "CQ TEST N9UNX" character by character
4. W1ABC responds on Radio 1 (left ear)
5. User types "W1ABC", presses Enter → Exchange sent on Radio 1
6. TX indicator shows "W1ABC 5NN 05" on Radio 1
7. **While Radio 1 is transmitting**, user presses Insert to swap focus to Radio 2
8. User waits, watching Radio 1's TX indicator for completion
9. Radio 1 TX completes, W1ABC sends exchange (heard in left ear)
10. User presses F1 → CQ sent on Radio 2 (Radio 1 is not TXing, no interruption)
11. TX indicator shows "CQ TEST N9UNX" on Radio 2
12. **While Radio 2 is transmitting**, user presses Insert to swap focus to Radio 1
13. User copies W1ABC's exchange into Radio 1's exchange field
14. K4XYZ responds on Radio 2 (heard in right ear)
15. Radio 2 TX completes
16. User presses Enter on Radio 1 → TU sent, QSO logged
17. User presses Insert to focus Radio 2
18. User types "K4XYZ", presses Enter → Exchange sent on Radio 2
19. Continue alternating...

---

## Notes from Previous Attempt

Issues encountered in v1 that this plan addresses:

1. **Single state machine tried to track both radios** → Now: Completely separate state machines per radio

2. **Audio didn't reflect independence** → Now: Dual mixer architecture, each radio has own mixer and noise

3. **TX indicator location unclear** → Now: Within each radio's panel, below input fields

4. **Backtick echoed to fields** → Now: Intercept before character processing

5. **Unclear how TX interruption should work** → Now: Explicit interruption handling with caller AGN behavior

6. **Statistics not separated** → Now: Per-radio tracking with combined totals
