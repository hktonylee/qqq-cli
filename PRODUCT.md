# qqq Product Context

## Register

product

## Users

Developers and coding agents managing project tasks from terminals. Human users
compose descriptions and attach screenshots; agents claim work through JSON CLI.

## Product Purpose

Local task queue with SQLite persistence, ownership, dependencies and history.
Keep task entry and execution close to coding sessions, without a server.

## Brand Personality

Practical, direct, terminal-native. Existing terminal layout and keyboard controls
carry the interface. Codex composer styling is the requested editor reference.

## Anti-references

Avoid decoration that interferes with text entry, hidden content, control sequences
from task text, or UI bytes leaking into machine-readable stdout.

## Design Principles

- Shared add/edit editor stays consistent across normal, blank and error states.
- Whole description remains intact through editing and paste.
- Visible shortcuts support keyboard operation; resizing keeps cursor reachable.
- Terminal state restores after save, cancel and errors.
- Plain output and existing color opt-outs remain available.

## Accessibility & Inclusion

Keep text readable on painted surfaces, including terminals with light default
foregrounds or backgrounds. Keyboard access and textual error feedback remain
available; color carries no required meaning. Respect `NO_COLOR` and `TERM=dumb`.
