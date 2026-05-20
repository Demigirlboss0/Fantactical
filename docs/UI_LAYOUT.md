# UI Layout

All UI rendering uses [`bevy_egui`](https://github.com/vladbat00/bevy_egui)
immediate-mode panels. Panel layout is defined in
`src/ui/panels.rs:render_panels()` (1055 lines).

## Screen Layout

```
┌─────────────────────────────────────────────────────────────┐
│ TopBottomPanel::top("phase_bar")           22px fixed       │
│ Round 3  |  Phase: ManeuverSelection  |  SOVEREIGN          │
├────────────────────────────────┬────────────────────────────┤
│                                │ SidePanel::right            │
│                                │ ("side_panel") 280px+       │
│        Bevy Viewport           │                             │
│        (battlemap)             │ ┌─────────────────────┐    │
│                                │ │ Character Panel     │    │
│                                │ │ ┌─[HP]──Francesca─┐ │    │
│  Tokens rendered as            │ │ │ HP ████░░ 14/18 │ │    │
│  flat-top hex sprites          │ │ │ FP ██░░░░  4/12 │ │    │
│  with portrait textures        │ │ │ Spd 6.25 Move 6  │ │    │
│                                │ │ └──────────────────┘ │    │
│  Maneuver relation arrows      │ │ ┌─[HP]──Nur────────┐ │    │
│  colored by category           │ │ │ HP ██████ 20/20  │ │    │
│                                │ │ └──────────────────┘ │    │
│                                │ ├─────────────────────┤    │
│                                │ │ [Attacks] [GM Conf] │    │
│                                │ │ Combat tabs...      │    │
├────────────────────────────────┴────────────────────────────┤
│ TopBottomPanel::bottom("maneuver_tray")  configurable height │
│ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐        │
│ │  Attack  │ │AOA Detrm │ │ Evaluate │ │   Aim    │  ...   │
│ │ Strike w │ │+4, no def│ │+1/turn  │ │+Acc afte│        │
│ └──────────┘ └──────────┘ └──────────┘ └──────────┘        │
├─────────────────────────────────────────────────────────────┤
│ TopBottomPanel::bottom("event_log")  configurable height     │
│ [R3] RollResult | Attack roll: 12 vs 15 — HIT              │
│ [R3] RollResult | Damage roll: 2d+1 cut = 9                │
│ [R3] InjuryResolved | Francesca to Torso — 6 hp lost        │
└─────────────────────────────────────────────────────────────┘
```

## Panel Details

### Phase Bar (`src/ui/panels.rs:124`)
- Fixed 22px, non-resizable
- Shows: Round number, current phase, current actor name (accent color),
  cursor hex coordinates, range in yards

### Maneuver Tray (`src/ui/panels.rs:156`)
- `TopBottomPanel::bottom`, configurable height (default 190px via settings)
- Non-resizable at runtime
- Horizontal `ScrollArea` with maneuver cards
- Cards use `draw_maneuver_card()` (`src/ui/panels.rs:925`) — 125×150px
  trading-card style with:
  - 5px solid colored border (category color)
  - Larger name header (13px font)
  - Separator line
  - Rules description (10px font)
  - Hover effect brightens border; selected uses accent color

### Character Panel (`src/ui/panels.rs:254`)
- `SidePanel::right`, min 280px, resizable at runtime
- Turn-order list of actor cards with:
  - Portrait (loaded from GCS, cached as egui texture)
  - Name, HP/FP bars (dynamic color), ST/DX/IQ/HT
  - Speed, Move, SM, Dodge, Parry
  - Posture, Encumbrance, position
  - Status flags (Stunned, Knocked Down, etc.)
  - Up/Dn reorder buttons
- Right-click opens inline dropdown: Reload Sheet, Remove Actor, Change Posture
- Bottom tabs: Attacks (lists current actor's attacks), GM Config

### GM Config (`src/ui/panels.rs:571`)
- Shock toggle checkbox
- Rewind Turn button
- Global modifier list with remove buttons
- Add modifier (label + value inputs)
- Lighting presets (Dim -2, Dark -4, Blind -10)
- Per-actor settings: pain threshold combo box, individual modifier list

### Event Log (`src/ui/panels.rs:358`)
- `TopBottomPanel::bottom`, configurable height (default 120px via settings)
- Non-resizable at runtime
- Reverse-chronological scrollable list
- Color-coded by `LogEntryKind`: Error (red), Warning (yellow), others (grey)
- Format: `[R{round}] {kind:?} | {message}`

## Roll Modal (`src/ui/roll_modal.rs`)
- Centered `egui::Window`, non-collapsible, non-resizable
- Content varies by phase:

| Phase | Content |
|-------|---------|
| AttackSetup | Attack selector (ComboBox with name + damage), hit location selector (28 options with penalties), target name, Confirm/Cancel |
| ManeuverConfirmed | Effective skill display, modifier breakdown list, Proceed/Cancel |
| AttackRoll | "Roll 3d6!" button, outcome text, damage result |
| DefenseResolution | Defender name, defense options (Dodge/Parry/Block with values), Skip Defense, Cancel |

## Battlemap (`src/ui/battlemap.rs`)
- Bevy 2D viewport (Camera2d) — NOT an egui panel
- Flat-top hex grid rendered via Gizmos (`draw_grid()` at line 800)
- Token entities spawned/updated by `sync_tokens()` (line 654):
  - Hex mesh with portrait texture
  - HP bar as colored sprite below the hex
  - Red flash background when HP is below max
- Maneuver relation arrows (`draw_relation_arrows()` at line 918):
  - Colored by category (red=offensive, orange=undefended, yellow=setup,
    blue=defensive, purple=mental)
  - Dashed for Wait maneuvers
- Camera controls (`camera_controls()` at line 866):
  - WASD / arrow keys pan
  - Middle-mouse drag pan
  - Scroll zoom

## Drag-and-Drop

### Maneuver Cards
- Click a card to arm it; click again to disarm
- While armed, a ghost card follows the cursor
- Release over an enemy hex → `ManeuverDeclaredEvent` with target
- Release over own hex / anywhere (self-targeted) → no target
- ESC cancels the drag

### Token Drag (`src/ui/battlemap.rs:detect_drag_input()` at line 566)
- Runs in `First` schedule (before egui rendering)
- Left press near a token → starts drag
- Left release over a hex → `GmActionEvent::MoveActor`
- Blocked when a maneuver card drag is active

## Coordinate System

All cursor→world conversions use logical pixels consistently:
- `window.cursor_position()` returns logical position
- `window.width()` / `window.height()` return logical size
- `hex_under_cursor()` (`src/ui/panels.rs:33`) converts screen coords to hex
  using flat-top axial math (inverse of `world_position()`)
- `world_position()` (`src/ui/battlemap.rs:413`) converts hex coords to world
  coords for token rendering
