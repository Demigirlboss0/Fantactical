# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added
- Implement WebSocket client with reconnect logic for network multiplayer (#19)
- Implement authoritative WebSocket server for network multiplayer sync (#18)
- Add comprehensive test suite for WebSocket networking (serialization, message roundtrip, integration) (#65)
- Implement WebSocket client with reconnect logic for network multiplayer (#64)
- Implement authoritative WebSocket server for network multiplayer sync (#63)
- Implement drag-to-move tokens on battlemap with hex grid snapping (#34)
- Implement turn order drag-to-reorder in character panel with multi-turn badges (#27)
- Add GM ability to move tokens on the battlemap and change actor posture (#28)
- Redesign right-click context menu as inline dropdown pushing cards below, styled like character cards (#26)
- Complete functional GM config panel with modifier management, rewind, shock toggle, and pain threshold controls (#11)
- Add right-click character sheet hot-reload modal styled to match app theme (#10)
- Add GUI sheet import button accessible from within the app (#9)
- Add proper error logging infrastructure with in-app event log and terminal output (#8)
- Wire turn phase state machine, maneuver tray, event log, GM config, and hex coordinate overlay (#7)
- Portrait on tokens via hex mesh, HP bar borders, red missing-HP background, hex orientation fix (#6)
- Hex grid with proper axial coordinates and axial distance (#4, #5)
- Battlemap viewport with grid, token spawning, camera pan/zoom (#3)
- UI panel layout with bevy_egui (character panel, event log, maneuver tray, tabs) (#4)
- GCS v5 JSON import with portrait, current HP/FP, weapons, armor, skills (#1)
- Injury resolution pipeline with full hit-location × damage-type matrix (#1)
- Maneuver legality checker with posture/status/encumbrance filters (#1)
- Dice rolling, crit hit/miss tables (B556/B557), half-skill crit checks (#1)
- State history persistence with save/load and per-round file naming (#2)
- Mil-sim theming with Theme trait, egui style injection (#2, #4)
- Bevy ECS scaffolding with resources, plugins, systems (#1–#6)

### Fixed
- Seed RNG with hashed system time to eliminate determinism (#62)
- Add dice roll results to event log for attack, defense, and damage rolls (#61)
- Fix All-Out Attack not disabling active defenses for the attacker (#60)
- Fix attack resolution to apply injury when defense is skipped (#59)
- Fix GCS import to handle ranged attacks and all melee weapon types (#58)
- Fix viewport offset in hex_under_cursor causing massive spatial mismatch between token render position and click detection (#35)
- Fix turn order reorder buttons showing invalid Unicode boxes instead of arrows (#33)
- Fix token right-clicks not registering on battlemap (#32)
- Fix battlemap not updating token positions when moved via MoveActor (#31)
- Fix token right-click dropdown opens then immediately closes in same frame (#29)
- Fix battlemap token right-click not triggering context menu (#25)
- Fix character card text overflowing boundaries and overlapping between multiple actor cards (#24)
- Fix turn order not sorting by Basic Speed and Enhanced Time Sense on import (#23)
- Fix GM config panel staying stale after sheet import (#22)
- Fix right-click context menu not showing on character cards and battlemap tokens (#21)
- Hex grid tessellation corrected from broken offset coords to proper axial coords (#5)
- Per-frame WebP decode eliminated, replaced with cached egui texture (#6)
- HP bar switched from Gizmos (invisible fill at small scale) to solid sprite (#6)
- HP current now reads `calc/current` from GCS instead of defaulting to max (#6)

### Changed
