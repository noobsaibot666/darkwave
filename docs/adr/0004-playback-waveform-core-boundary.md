# ADR 0004: Playback and waveform core boundary

## Status

Accepted

## Context

Milestone 2 requires fast auditioning, previous/next playback, seeking, looping, waveform peak generation, virtualized browser rows, and a persistent transport. The full audio output stack will require decoder, device, and platform integration, but core playback state and waveform cache behavior can be tested independently first.

## Decision

Introduce a small playback session state machine in `audio-engine` and deterministic waveform peak generation in `waveform`.

The playback session owns:

- Active asset identity.
- Duration.
- Position.
- Playing state.
- Optional loop region.
- Load semantics that cancel prior playback and reset position.

The waveform crate owns:

- Sample-to-peak conversion with sample clamping.
- Peak downsampling.
- Multi-resolution waveform cache payloads for row, inspector, and transport renderers.

## Consequences

- UI and future decoder integration can share one playback state contract.
- Rapid row changes can be represented as asset load events that stop prior playback before output integration exists.
- Waveform rendering can consume precomputed peak payloads without decoding full files in list rows.
- Real audio decoding, output device selection, and under-100 ms playback benchmarking remain future Milestone 2 work.
