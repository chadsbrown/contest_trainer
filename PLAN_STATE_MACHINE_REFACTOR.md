# Plan: State Machine Refactoring

## Background

This discussion emerged during 2BSIQ planning. The current state machine has limitations that affect both single-radio and 2BSIQ modes. This refactoring should be completed **before** implementing 2BSIQ, as it may have consequences for that plan.

## Problem Statement

### Current State Machine is Action-Driven

The current `ContestState` enum enforces a rigid linear flow where specific keys only work in specific states:
- F2 (send exchange) only valid in certain states
- F5 (send his call) only works during `StationsCalling`
- The state machine dictates what the user can do at each moment

This creates problems when:
1. **User interrupts their own transmission** (especially relevant in 2BSIQ, but also single-radio)
2. **User wants to recover from a mistake**
3. **User wants to resend information the caller didn't copy**

### Real-World Flexibility Needed

In a real contest, the user should be able to press F2/F5/F8 at almost any time. The state machine should track **what information has been successfully communicated** rather than enforcing a rigid sequence.

**Example scenario:**
- User types callsign, presses Enter (sends call + exchange)
- Transmission gets interrupted (in 2BSIQ: user started TX on other radio)
- User presses F5 (sends caller's callsign)
- User presses F2 (sends exchange)
- This should be equivalent to the original Enter, and the QSO can continue

## Proposed Approach: Information-Driven State Machine

### Track What Has Been Communicated

Instead of (or in addition to) states like `SendingExchange`, track the actual information flow:

```rust
pub struct QsoProgress {
    // What has the caller successfully received from us?
    pub caller_heard_their_call: bool,   // We sent their callsign (F5 or via Enter)
    pub caller_heard_our_exchange: bool, // We sent our exchange (F2 or via Enter)
    
    // What have we successfully received from caller?
    pub we_have_their_call: bool,        // User entered something in call field
    pub we_have_their_exchange: bool,    // User entered something in exch field
    
    // TX interruption tracking
    pub last_tx_interrupted: bool,
    pub last_tx_contained_call: bool,    // Did interrupted TX include their call?
    pub last_tx_contained_exchange: bool, // Did interrupted TX include our exchange?
}
```

### Caller Behavior Based on Information Received

The caller's response depends on what they've successfully received:

| Caller Heard Call? | Caller Heard Exchange? | Caller Response |
|--------------------|------------------------|-----------------|
| No | No | Confused - sends call again, or "?" |
| Yes | No | Sends "AGN" or "?" |
| Yes | Yes | Sends their exchange |

### User Can Send Anytime

- **F5** → sends caller's callsign, sets `caller_heard_their_call = true` (if TX completes)
- **F2** → sends our exchange, sets `caller_heard_our_exchange = true` (if TX completes)
- **Enter** in call field → sends call + exchange (equivalent to F5 then F2)
- **F8** → sends AGN/?, requests repeat from caller

### Interruption Handling

If TX is interrupted before completion:
- The information in that TX is considered **not received** by caller
- Caller will respond accordingly (AGN, ?, confusion)
- User can retry with F5, F2, or other keys

## Open Questions

### 1. Granularity of "Heard"

If user sends "W1ABC 5NN 05" and gets interrupted after "W1ABC 5N", has the caller:

**Option A - Pessimistic**: Any interruption invalidates entire message
- Simple to implement
- Caller sends AGN for any interruption

**Option B - Parse what was sent**: Track which elements completed
- More complex
- If callsign completed but exchange didn't, caller knows who but needs exchange

**Option C - Probabilistic**: Sometimes they get partial, sometimes not
- Most realistic
- More complex to implement and test

**Recommendation**: Start with Option A (pessimistic). Can refine later if needed.

### 2. F5 Behavior in Different Contexts

Currently F5 has different meanings:
- During `StationsCalling` with partial in field: Query partial (filter callers)
- Other contexts: Send his callsign

Should F5 behavior be:
- **Unified**: Always sends whatever is in the call field
- **Context-dependent**: Query partial vs. send callsign based on state
- **Separate keys**: Different key for partial query vs. send callsign

This needs user input to decide.

### 3. What States Are Still Needed?

Even with information-driven tracking, we still need states for:
- `Idle` - nothing happening
- `CallingCq` - user is sending CQ
- `WaitingForCallers` - CQ done, waiting for responses
- `StationsCalling` - callers are transmitting
- `UserTransmitting` - user is sending something (what?)
- `CallerTransmitting` - caller is sending something (exchange, AGN, correction)
- `QsoComplete` - QSO logged, sending TU

But transitions become more flexible based on `QsoProgress`.

### 4. Interaction with Existing Features

How does this affect:
- **Call correction flow**: Caller corrects wrong callsign
- **AGN request flow**: Caller sends AGN/?
- **Tail-ender flow**: New caller jumps in after QSO
- **Partial query**: Filtering multiple callers

### 5. Current State Enum

The current `ContestState` has 27 states. How many can be collapsed or simplified with the information-driven approach? Or do we keep the states but make transitions more flexible?

## Relationship to 2BSIQ

### Why This Matters for 2BSIQ

In 2BSIQ mode, TX interruption is common:
- User is sending on Radio 1
- User needs to send on Radio 2
- Radio 1's TX is interrupted
- User needs to recover gracefully on Radio 1 later

Without the information-driven approach, recovering from interruption is awkward or impossible.

### Order of Implementation

1. **First**: Refactor state machine for single-radio mode
2. **Second**: Implement 2BSIQ on top of improved state machine

This ensures the foundation is solid before adding complexity.

## Next Steps

1. Review current `ContestState` enum and all 27 states
2. Identify which states can be simplified with information tracking
3. Decide on F5 behavior (unified vs. context-dependent)
4. Design the new state machine / QsoProgress hybrid
5. Implement and test in single-radio mode
6. Proceed with 2BSIQ implementation

## Files to Review

- `src/app.rs` - Current state machine implementation
- `docs/STATE_MACHINE.md` - Current state documentation
- `src/station/caller_manager.rs` - Caller behavior that responds to user actions

## Discussion Notes

This plan originated from 2BSIQ planning discussion. Key insight: the user asked "there is a general problem with the state machine... the user should probably always be able to press F2/F5/F8. We need a way for the state machine to understand if the user has sent enough information to the caller to be able to continue or not."

The example given: if user interrupts transmission containing caller's callsign and exchange, then later presses F5 (his call) then F2 (exchange), it should be equivalent to the original Enter - the QSO should be able to continue normally.
