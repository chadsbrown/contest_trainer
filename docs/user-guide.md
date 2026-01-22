# CW Contest Trainer - User Documentation

A Morse code contest simulator for practicing CW contest operating skills.

## Quick Start

1. Launch the application
2. Press **F1** or **Enter** to call CQ
3. Copy the callsign of responding stations into the **Call** field
4. Press **Tab** to move to the **Exch** field and enter the received exchange
5. Press **Enter** to log the QSO
6. Repeat!

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| F1 | Send CQ |
| F2 | Send your exchange |
| F3 | Send TU (thank you) |
| F5 | Send his callsign |
| F8 | Request repeat (AGN/?) |
| F12 | Wipe/clear current QSO |
| Enter | Submit current field |
| Tab | Move between Call and Exchange fields |
| Esc | Clear current field |

## Settings

Access settings via **File > Settings**. Settings are automatically saved to your system's config directory.

---

## User Settings

### Your Callsign
- **Purpose**: Your amateur radio callsign used during the simulated contest
- **Default**: `N9UNX`
- **Values**: Any valid callsign string

### Your Name
- **Purpose**: Your operator name, sent as part of exchange in contests that require it (e.g., NA Sprint, CWT)
- **Default**: `OP`
- **Values**: Any name string (automatically uppercased)

### CQ Zone
- **Purpose**: Your CQ zone number for CQ WW contest exchanges
- **Default**: `5`
- **Values**: 1-40

### Section/Exchange
- **Purpose**: Your ARRL section or exchange value for contests like Sweepstakes
- **Default**: `CT`
- **Values**: Valid ARRL section abbreviation or exchange string

### Your WPM
- **Purpose**: The speed at which your CW transmissions are sent
- **Default**: `32`
- **Values**: 15-50 WPM

### Font Size
- **Purpose**: UI font size for the application
- **Default**: `14.0`
- **Values**: 10.0-24.0

### AGN Message
- **Purpose**: The message sent when you request a repeat from a station
- **Default**: `?`
- **Values**: Typically `?` or `AGN`

### Show Status Line
- **Purpose**: Toggle visibility of the status indicator showing current contest state
- **Default**: `true` (enabled)
- **Values**: true/false

---

## Contest Settings

### Contest Type
- **Purpose**: Select the contest format to simulate
- **Default**: `CWT`
- **Values**:
  - **CQ World Wide (CqWw)**: Exchange is RST + CQ Zone (e.g., `599 05`)
  - **NA Sprint (NaSprint)**: Exchange is serial number + name + state (e.g., `123 BOB TX`)
  - **ARRL Sweepstakes**: Exchange is serial + precedence + check + section (e.g., `42 A 99 CT`)
  - **CWT**: Exchange is name + number or name + state (e.g., `BOB 123` or `JOE TX`)

### Callsign File
- **Purpose**: Path to the file containing callsigns for simulated stations
- **Default**: `callsigns.txt`
- **Values**: Path to a text file with one callsign per line

### CWT Callsign File
- **Purpose**: Path to the file containing callsigns specifically for CWT contests (includes name/number data)
- **Default**: `cwt_callsigns.txt`
- **Values**: Path to a CWT-formatted callsign file

### CQ Message
- **Purpose**: The CQ message sent when calling CQ
- **Default**: `CQ TEST`
- **Values**: Any CQ message string

---

## Simulation Settings

### Max Simultaneous Stations
- **Purpose**: Maximum number of stations that can call you at once (pile-up simulation)
- **Default**: `2`
- **Values**: 1-5

### Station Probability
- **Purpose**: Probability that a station will respond after your CQ
- **Default**: `0.7` (70%)
- **Values**: 0.1-1.0

### WPM Range (Min/Max)
- **Purpose**: Speed range for simulated calling stations
- **Default**: `28-36` WPM
- **Values**: 10-50 WPM (min must be <= max)

### Frequency Spread (Hz)
- **Purpose**: How far off your frequency stations may call (simulates real band conditions)
- **Default**: `400` Hz
- **Values**: 0-1000 Hz

### Signal Strength Range (Min/Max)
- **Purpose**: Amplitude range for simulated station signals (simulates varying signal strengths)
- **Default**: `0.4-1.0`
- **Values**: 0.1-1.0 (min must be <= max)

### Caller Needs Repeat Probability
- **Purpose**: Probability that a calling station will request you repeat your exchange (sends AGN or ?)
- **Default**: `0.1` (10%)
- **Values**: 0.0-1.0

---

## Audio Settings

### Tone Frequency (Hz)
- **Purpose**: The pitch of the CW sidetone
- **Default**: `600` Hz
- **Values**: 400-1000 Hz

### Noise Level
- **Purpose**: Base level of background static/white noise
- **Default**: `0.15`
- **Values**: 0.0-0.5

### Noise Bandwidth (Hz)
- **Purpose**: Simulates receiver CW filter bandwidth. Narrower values create more focused, resonant noise (like a tight CW filter); wider values create fuller, more broadband noise.
- **Default**: `400` Hz
- **Values**: 100-1000 Hz

### Master Volume
- **Purpose**: Overall audio volume
- **Default**: `0.7`
- **Values**: 0.0-1.0

### Mute Background Noise During TX
- **Purpose**: Silence background noise while your CW is being sent (makes your transmissions clearer)
- **Default**: `true` (enabled)
- **Values**: true/false

---

## Static/QRN Settings

These settings simulate realistic band noise conditions.

### Crash Rate
- **Purpose**: Frequency of static crashes (lightning-like noise bursts)
- **Default**: `0.3` per second
- **Values**: 0.0-2.0 per second (0.0 disables)

### Crash Intensity
- **Purpose**: Volume/strength of static crashes
- **Default**: `0.4`
- **Values**: 0.0-1.0

### Pop Rate
- **Purpose**: Frequency of clicks and pops
- **Default**: `2.0` per second
- **Values**: 0.0-10.0 per second (0.0 disables)

### Pop Intensity
- **Purpose**: Volume/strength of pops and clicks
- **Default**: `0.3`
- **Values**: 0.0-1.0

### QRN Intensity
- **Purpose**: Level of atmospheric noise rumble (low-frequency noise)
- **Default**: `0.2`
- **Values**: 0.0-1.0

---

## Main Window Controls

### Reset Stats
Clears all QSO counts, points, and session statistics.

### Toggle Static (ON/OFF)
Enables or disables background noise and QRN effects.

### Session Stats
Opens a detailed statistics window showing:
- Total QSOs and accuracy rates
- Callsign and exchange accuracy breakdown
- AGN usage statistics
- Calling station WPM analysis
- Character error analysis (identifies which characters you struggle with)
- Recent QSO history

---

## Exchange Formats by Contest

| Contest | Exchange Format | Example |
|---------|----------------|---------|
| CQ WW | RST + Zone | `599 05` |
| NA Sprint | Serial + Name + State | `123 BOB TX` |
| Sweepstakes | Serial + Prec + Check + Section | `42 A 99 CT` |
| CWT | Name + Number (or Name + State) | `BOB 123` or `JOE TX` |

---

## Realism Behaviors

The simulator includes several behaviors that model real contest operating conditions.

### Call Correction

When you enter the wrong callsign for a station, the calling station may or may not correct you:

- **~80% of the time**: The station will send a correction
- **~20% of the time**: The station will just proceed with the wrong call (you'll get a "busted call" penalty)

When a station does correct you, they use one of two formats:
- **~85% of corrections**: Send their callsign twice for emphasis (e.g., "W1ABC W1ABC")
- **~15% of corrections**: Send their callsign once followed by their exchange (e.g., "W1ABC 5NN 123")

A station will attempt to correct you up to 2 times before giving up.

These behaviors are controlled by the `[simulation.call_correction]` settings.

### Caller Persistence and Retry Behavior

Stations in the pileup don't robotically call every single CQ. Each caller has a "patience" level (2-5 attempts) determining how many times they'll try before giving up.

**Callers don't always call back-to-back.** Two mechanisms create realistic intermittent calling:

1. **Retry Delay**: After each CQ, callers wait a random delay (200-1200ms) before they're ready to call again. If you send your next CQ quickly, some callers won't be ready yet.

2. **Call Probability**: Even when ready, callers may "sit out" a round:
   - Patience 2: 60% chance to call each round
   - Patience 3: 70% chance to call each round
   - Patience 5: 90% chance to call each round

This models real operator behavior—pausing to tune around, waiting for the pileup to thin, or timing their call strategically.

More patient callers are more persistent, but even they exhibit natural variation. A caller with patience 3 might call on rounds 1 and 3, skipping round 2 entirely.

These behaviors are controlled by the `[simulation.pileup]` settings.

---

## Configuration File

Settings are stored in TOML format at:
- **Linux**: `~/.config/contest_trainer/settings.toml`
- **macOS**: `~/Library/Application Support/contest_trainer/settings.toml`
- **Windows**: `%APPDATA%\contest_trainer\settings.toml`

Settings are automatically saved when changed in the UI.
