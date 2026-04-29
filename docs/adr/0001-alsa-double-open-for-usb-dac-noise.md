# ADR 0001 — Always double-open the ALSA PCM to avoid noise on rate-changing USB DACs

- Date: 2026-04-29
- Status: Accepted
- Validated by: PapyKahan
- Scope: ALSA backend (`src/audio/api/alsa/`)

## Context

When playback starts with the ALSA backend on a raw `hw:X,Y` device, some USB
DACs emit continuous noise (a constant *grésillement*, not isolated clicks)
for the entire track. This was reproduced systematically on a Colorfly CDA
M1P (`usb 2fc6:f827`) under the following conditions:

- First playback after rhap startup (the device was last configured by
  PipeWire at `s32le 48 kHz`).
- Any track change where the new track has a different sample rate from the
  one the PCM was last configured with.

A second open of the same track — i.e. the user stops playback and presses
play again — always plays cleanly. Track changes where the new sample rate
matches the previous one also play cleanly.

Other USB DACs (Onix Alpha XI1) do **not** exhibit the issue: their first
playback after a rate change is clean.

## Investigation summary

Several hypotheses were tested against the failing M1P:

| Hypothesis                                     | Diagnostic                              | Result                       |
| ---------------------------------------------- | --------------------------------------- | ---------------------------- |
| ALSA write path corrupts bytes                 | Audio thread writes only zeros          | Pure silence — write OK      |
| Buffer underrun (XRUN) loop                    | XRUN warn logs on `writei`/`avail_update` | No XRUN observed           |
| Format / frame-size mismatch                   | Log negotiated format vs. requested     | Always matched               |
| Bigger buffer / higher start_threshold         | 8 periods, threshold = `buffer-period`  | No improvement               |
| Longer `stop()` → reopen grace period          | 100/500/800 ms sleep                    | No improvement               |
| `plughw:` instead of `hw:`                     | ALSA plug layer                         | Still noisy                  |
| Pre-write silence to let the DAC PLL settle    | 100/300/800 ms of zeros before audio    | No improvement               |
| Audio thread synthesizes a sine directly       | Bypasses ring buffer entirely           | Noisy on rate change         |

The decisive observation is the last one: noise is present even when the
audio thread synthesizes the signal locally and the ring buffer / decoder /
producer are all bypassed. Combined with the user's repro pattern
(*rate change → noise; same rate → clean*), the cause is below our code:
the kernel `snd_usb_audio` driver renegotiates the USB altset on a rate
change, and the affected DACs need a full PCM close/reopen cycle for the
new clock state to settle. Pre-writing silence does not help because the
device is not actually waiting on data — it is waiting on the next
`snd_pcm_open` to take effect.

This matches the manual workaround the user already had: stop, then play
again. The second `snd_pcm_open` finds the rate already negotiated and
starts cleanly.

## Decision

In the ALSA backend, `acquire_pcm` always performs **two** open/close
sequences:

1. A sacrificial open with the requested params, immediately closed.
2. A 10 ms sleep so the kernel finishes releasing URBs before the next open.
3. The real open returned to the caller.

This is unconditional — no CLI flag, no per-device list. The cost is one
extra ALSA open + close + 10 ms per track start (~20 ms wall-clock total in
practice), which is below perceptual threshold for "press play → audio".
The benefit is that every supported USB DAC plays cleanly from the first
sample, including the ones that need this and the ones that don't.

The 10 ms delay was chosen as the smallest value that survived jitter in
testing; values much smaller (0–1 ms) sometimes left the DAC in the noisy
state. Values larger than 10 ms add latency for no observed benefit on the
hardware tested.

## Consequences

- All ALSA track starts now go through two open/close sequences.
- We pay ~20 ms of latency per track start. Acceptable.
- We pay one extra `snd_pcm_open` syscall pair per track start. Negligible.
- Devices that did not need this (Onix XI1, presumably most others) are
  unaffected behaviorally; they just see the extra open as a no-op.
- If a future device has a problem with the double-open (e.g. EBUSY between
  the two opens), the existing `acquire_with_backoff` retry loop already
  handles it.

## Alternatives considered

- **Per-device opt-in CLI flag (`--prime-dac`)**: rejected. The symptom is
  a non-obvious crackle that users would not link to a flag they did not
  set. Surprise-free defaults matter more than ~20 ms of latency.
- **USB-ID allowlist**: rejected. Maintaining a list of "known-affected"
  DACs in source is a long-tail support burden, and false negatives ship
  silently.
- **Pre-write silence**: tested up to 800 ms. Does not fix the issue; the
  PLL state is reset by the close/reopen, not by data.

## References

- Repro thread / debugging session that produced this ADR (April 2026).
- `src/audio/api/alsa/device.rs::acquire_pcm` is the implementation.
