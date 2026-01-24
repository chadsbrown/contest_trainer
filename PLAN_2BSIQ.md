# Plan: 2BSIQ (Two Radio) Mode Implementation

## Overview

Add an optional 2BSIQ mode where the user operates two radios simultaneously:
- **Radio 1**: Left ear (left audio channel)
- **Radio 2**: Right ear (right audio channel)
- User switches focus between radios to work independent QSOs

## Keyboard Controls (N1MM+ Style)

Based on [N1MM+ Key Assignments](https://n1mmwp.hamdocs.com/setup/keyboard-shortcuts/):

| Key | Action |
|-----|--------|
| **Pause** | Swap radios - move both TX and RX focus to other radio |
| **` (backtick/tilde)** | Toggle stereo mode on/off |
| **Ctrl+Left Arrow** | Move TX and RX focus to Radio 1 (left) |
| **Ctrl+Right Arrow** | Move TX and RX focus to Radio 2 (right) |
| **Ctrl+F1 to Ctrl+F8** | Send Fn message on alternate (non-focused) radio |

**Existing keys (F1, F2, F3, F5, F8, Enter, Tab, etc.)** apply to the focused radio.

## Audio Behavior (Decided)

### Stereo Mode Toggle (Backtick)
- **Stereo ON**: Radio 1 → Left ear only, Radio 2 → Right ear only (true 2BSIQ)
- **Stereo OFF**: Focused radio → Both ears (concentrate on one radio)

### User TX Sidetone
- **No sidetone in 2BSIQ mode** - user does not hear their own transmission
- Instead, a **visual TX indicator** shows transmission progress (see UI section)

### Noise
- **Independent noise per radio** - each radio has its own noise generator for realism

### Latch Mode (Optional)
- **When enabled**: During transmission on one radio, the *other* radio's audio is routed to both ears
- **When transmission ends**: Audio returns to normal stereo mode (if stereo enabled)
- **Purpose**: Allows user to focus on receiving on the non-transmitting radio while sending
- **Setting**: Configurable on/off in settings

**Example behavior with latch enabled:**
1. User is in stereo mode (R1→left, R2→right)
2. User starts transmitting on Radio 1
3. During TX: Radio 2 audio → both ears (Radio 1 silent, no sidetone)
4. TX ends: Returns to stereo (R1→left, R2→right)

## Key Architectural Changes

### 1. Audio System (Stereo Output)

**Current**: Mono audio duplicated to both channels
**Required**: True stereo with Radio 1 → Left, Radio 2 → Right

**Files to modify**:
- `src/audio/mixer.rs` - Create `DualMixer` or add stereo support
- `src/audio/engine.rs` - Update `build_stream()` for stereo routing
- `src/messages.rs` - Add `radio_index` to `StationParams`

**Changes**:
```rust
// messages.rs - Add radio routing
pub struct StationParams {
    // ... existing fields ...
    pub radio_index: u8,  // 0 = Radio 1 (left), 1 = Radio 2 (right)
}

// mixer.rs - Dual mixer structure
pub struct DualMixer {
    pub radio1: Mixer,  // Left channel (with own NoiseGenerator)
    pub radio2: Mixer,  // Right channel (with own NoiseGenerator)
    pub stereo_enabled: bool,  // Toggle via backtick
    pub focused_radio: u8,     // Which radio to hear when stereo off
    // No user_station in 2BSIQ - no sidetone
}
```

### 2. Visual TX Indicator

**New requirement**: Show transmission text synchronized with audio timing.

**Display below input fields**:
```
│ Call: [W1ABC___]   Exch: [5NN 123_]             │
│ TX: CQ TEST N▌                                  │
```

The TX text reveals character-by-character in sync with when that character would be heard in audio. This requires:

1. Track current transmission position in `UserStation` or similar
2. Expose "characters sent so far" to UI
3. Render partial message with cursor/block at current position

**Implementation approach**:
- `UserStation` already tracks `current_element_idx` and `samples_elapsed`
- Need to map element index back to character position
- Add `get_characters_sent(&self) -> usize` method
- UI queries this each frame and displays `message[0..chars_sent]`

**Files to modify**:
- `src/audio/mixer.rs` - Add method to get TX progress
- `src/ui/main_panel.rs` - Render TX indicator below input fields

### 3. State Machine (Parallel Radio States)

**Current**: Single `ContestState` enum
**Required**: Independent state per radio

**Files to modify**:
- `src/app.rs` - Add dual state management

**Changes**:
```rust
pub struct RadioState {
    pub state: ContestState,
    pub callsign_input: String,
    pub exchange_input: String,
    pub current_field: InputField,
    pub caller_manager: CallerManager,
    pub current_tx_message: Option<String>,  // For TX indicator
}

pub struct ContestApp {
    // 2BSIQ mode fields
    pub two_bsiq_enabled: bool,
    pub radio1: RadioState,
    pub radio2: RadioState,
    pub focused_radio: RadioId,  // Which radio has keyboard focus
    pub stereo_enabled: bool,    // Audio routing mode
}

pub enum RadioId { One, Two }
```

### 4. UI Changes (Dual Radio Display)

**Layout concept**:
```
┌─────────────────────────────────────────────────┐
│ Contest Type                    [2BSIQ Mode]    │
│ QSOs: XX  Points: XXX  Rate: XX/hr  WPM: XX     │
├────────────────────┬────────────────────────────┤
│ ▶ RADIO 1 (LEFT)   │   RADIO 2 (RIGHT)          │
│ Status: Sending CQ │   Status: Idle             │
│ Call: [________]   │   Call: [________]         │
│ Exch: [________]   │   Exch: [________]         │
│ TX: CQ TEST N▌     │                            │
├────────────────────┴────────────────────────────┤
│ F1:CQ F2:Exch F3:TU | Pause:Swap  ~:Stereo      │
│ [STEREO: ON]  [FOCUS: Radio 1]                  │
└─────────────────────────────────────────────────┘
```

- Focused radio highlighted (border, background, or arrow indicator)
- Both radios always visible
- **TX indicator** shows below input fields, synchronized with audio timing
- Status indicators for stereo mode and current focus

### 5. Caller Management

**Separate CallerManager per radio** - each radio has independent caller pool, stations spawn independently on each radio.

## Implementation Phases (Session-Friendly Milestones)

Each phase is self-contained with a clear verification step. Can be completed across multiple sessions.

---

[X] Phase 1: Stereo Audio Foundation (completed)
**Goal**: Get basic stereo output working (Radio 1 → left, Radio 2 → right)

**Changes**:
1. Add `radio_index: u8` to `StationParams` in `src/messages.rs`
2. Modify `Mixer` in `src/audio/mixer.rs` to output stereo samples
3. Update `AudioEngine::build_stream()` in `src/audio/engine.rs` for stereo

**Verification**: 
- Manually assign test stations to radio_index 0 or 1
- Confirm left/right channel separation in headphones
- Existing single-radio mode still works (backward compatible)

**Files**: `src/messages.rs`, `src/audio/mixer.rs`, `src/audio/engine.rs`

---

[x] Phase 2: Independent Noise Per Radio (completed)
**Goal**: Each radio has its own noise generator

**Changes**:
1. Create second `NoiseGenerator` instance in mixer
2. Mix noise into appropriate channel based on radio

**Verification**:
- Two distinct noise sounds in left vs right ear
- Noise settings still work

**Files**: `src/audio/mixer.rs`

---

[x] Phase 3: 2BSIQ Settings & Mode Toggle (completed)
**Goal**: Add configuration to enable 2BSIQ mode

**Changes**:
1. Add `two_bsiq_enabled: bool` to `UserSettings` in `src/config.rs`
2. Add `latch_mode: bool` to settings
3. Add checkboxes in `src/ui/settings_panel.rs`

**Verification**:
- Settings appear in UI
- Settings persist to config file
- No behavioral changes yet (just flags)

**Files**: `src/config.rs`, `src/ui/settings_panel.rs`

---

[x] Phase 4: RadioState Structure (completed)
**Goal**: Create the data structures for dual radio state

**Changes**:
1. Create `RadioState` struct in `src/app.rs`
2. Create `RadioId` enum
3. Add `radio1`, `radio2`, `focused_radio`, `stereo_enabled` fields to `ContestApp`
4. Initialize both radio states on startup (when 2BSIQ enabled)

**Verification**:
- App compiles and runs
- No behavioral changes yet (existing single-radio mode works)

**Files**: `src/app.rs`

---

[x] Phase 5: Dual Caller Management (completed)
**Goal**: Each radio has independent caller pool

**Changes**:
1. Move `CallerManager` into `RadioState`
2. Create separate caller manager per radio when 2BSIQ enabled
3. Spawn stations with appropriate `radio_index`

**Verification**:
- In 2BSIQ mode, stations appear on both radios independently
- Audio separation works (Radio 1 callers in left ear, Radio 2 in right)

**Files**: `src/app.rs`, `src/station/caller_manager.rs`

---

[x] Phase 6: Basic Dual Radio UI (completed)
**Goal**: Display both radios side-by-side

**Changes**:
1. Create `render_radio_panel()` function in `src/ui/main_panel.rs`
2. Render two panels when 2BSIQ enabled
3. Show focus indicator (arrow/highlight on active radio)
4. Separate input fields per radio

**Verification**:
- Both radios visible in UI
- Can see which radio is focused
- Input appears in correct radio's fields

**Files**: `src/ui/main_panel.rs`

---

[x] Phase 7: Radio Focus Keyboard Controls (completed)
**Goal**: Implement radio switching keys

**Changes**:
1. Implement **Pause** key to swap focus
2. Implement **Ctrl+Left/Right** for explicit radio selection
3. Route existing F1-F8, Enter, Tab to focused radio only

**Verification**:
- Pause swaps focus indicator
- Ctrl+arrows select specific radio
- F1 sends CQ on focused radio only

**Files**: `src/app.rs`

---

[x] Phase 8: Stereo Toggle (completed)
**Goal**: Implement backtick to toggle stereo mode

**Changes**:
1. Implement **backtick** key to toggle `stereo_enabled`
2. When stereo OFF: route focused radio to both ears
3. Add stereo status indicator to UI

**Verification**:
- Backtick toggles stereo
- Stereo OFF: focused radio heard in both ears
- Stereo ON: left/right separation

**Files**: `src/app.rs`, `src/audio/mixer.rs`, `src/ui/main_panel.rs`

---

[x] Phase 9: Parallel State Machines (completed)
**Goal**: Each radio runs independent ContestState

**Changes**:
1. Route state transitions to correct radio based on focus
2. Route `StationComplete` audio events to correct radio by ID
3. Each radio can be in different state simultaneously

**Verification**:
- Work QSO on Radio 1 while Radio 2 is idle
- Complete QSO on Radio 1, Radio 2 state unchanged
- Can have overlapping QSOs

**Files**: `src/app.rs`

---

### Phase 10: TX Visual Indicator
**Goal**: Show transmission progress without sidetone

**Changes**:
1. Add character-position tracking to `UserStation` in `src/audio/mixer.rs`
2. Expose "characters sent" count to UI
3. Render TX text below input fields, synchronized with audio
4. Disable sidetone when 2BSIQ enabled

**Verification**:
- No audio sidetone in 2BSIQ mode
- TX text appears character-by-character matching timing
- Can see what's being sent on each radio

**Files**: `src/audio/mixer.rs`, `src/ui/main_panel.rs`, `src/app.rs`

---

### Phase 11: Latch Mode
**Goal**: Optional mode to hear other radio during TX

**Changes**:
1. When latch enabled and transmitting on Radio X: route Radio Y to both ears
2. When TX ends: return to normal stereo (if enabled)

**Verification**:
- Enable latch mode in settings
- Start TX on Radio 1: hear Radio 2 in both ears
- TX ends: back to stereo

**Files**: `src/audio/mixer.rs`, `src/audio/engine.rs`

---

### Phase 12: Ctrl+Fn for Alternate Radio
**Goal**: Send function key messages on non-focused radio

**Changes**:
1. Implement Ctrl+F1 through Ctrl+F8
2. These send the corresponding message on the OTHER radio

**Verification**:
- Focus on Radio 1, press Ctrl+F1: CQ sent on Radio 2
- TX indicator shows on Radio 2

**Files**: `src/app.rs`

---

### Phase 13: Documentation & Polish
**Goal**: Update docs, clean up edge cases

**Changes**:
1. Update `docs/STATE_MACHINE.md` with 2BSIQ states
2. Add 2BSIQ section to user guide
3. Handle edge cases (mode switching mid-QSO, etc.)

**Verification**:
- Docs accurate
- No crashes on mode toggle
- Clean transitions

**Files**: `docs/STATE_MACHINE.md`, `docs/user-guide.md`

## Files to Modify Summary

| File | Changes |
|------|---------|
| `src/messages.rs` | Add `radio_index` to `StationParams` |
| `src/audio/mixer.rs` | Dual mixer, independent noise, TX progress tracking |
| `src/audio/engine.rs` | Stereo routing, stereo toggle, no sidetone in 2BSIQ |
| `src/app.rs` | RadioState, dual state management, keyboard routing |
| `src/ui/main_panel.rs` | Dual radio layout, TX indicator |
| `src/config.rs` | Add `two_bsiq_enabled` setting |
| `src/ui/settings_panel.rs` | Add 2BSIQ toggle |
| `src/station/caller_manager.rs` | Support per-radio caller spawning |
| `docs/STATE_MACHINE.md` | Document 2BSIQ states and flows |

## Verification

1. **Audio test**: Confirm Radio 1 audio only in left ear, Radio 2 only in right (stereo mode)
2. **Stereo toggle**: Backtick switches to focused radio in both ears
3. **No sidetone**: User TX produces no audio in 2BSIQ mode
4. **TX indicator**: Visual text matches audio timing character-by-character
5. **Independent noise**: Each radio has distinct noise
6. **Radio swap**: Pause key moves focus correctly
7. **State independence**: Work QSO on Radio 1 while Radio 2 in different state
8. **Ctrl+Fn**: Can send messages on non-focused radio
9. **Simultaneous QSOs**: Can complete two QSOs overlapping in time

## Sources

- [N1MM+ Key Assignments](https://n1mmwp.hamdocs.com/setup/keyboard-shortcuts/)
- [N1MM+ Single Operator Contesting](https://n1mmwp.hamdocs.com/manual-operating/single-operator-contesting/)
- [N1MM+ Function Keys](https://n1mmwp.hamdocs.com/setup/function-keys/)
