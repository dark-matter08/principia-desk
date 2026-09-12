# Design system — "the UI is the diagram"

Principia Desk retains the visual system on `main` (`73dfa512`): blueprint grids,
node panels, compact monospace controls, status LEDs, colored metadata badges,
numbered rails and dashed connections, with Fraunces headlines and a paper
reading surface. The user explicitly reaffirmed this direction on 2026-09-09
after rejecting the simplified preview. New functionality must extend this
visual system. Global navigation has four destinations: Today, Classes, Progress
and Settings. The user requested a searchable sidebar inside Classes and a large
detail pane with Overview, Settings, Starting point, Curriculum and Schedule tabs.
Each pane scrolls within the available desktop height; preserve drafts across
class/tab changes and keep the header and tabs accessible. Today combines class
appointments; the duplicate Schedule destination and daily routine are retired.

Use `main`'s `SetupWizard`, `Idle`, `NodeCard`, `MetaBadge`, `StatusLED`,
`TimePicker` and the screenshots under `docs/screenshots/` as references. The
new `FlowStage` extracts the numbered rail from that setup design. Course and
class terminology may become more consistent while the original visual
character remains intact. Diagrams and status indicators should represent
actual relationships and state.

## Vocabulary map

| App concept | UI vocabulary | Component |
|---|---|---|
| Setup wizard | bootstrap your training cluster | NodeCard topology + pipes |
| Class schedule | Study times / Plan a week inside the selected class | ClassSchedule |
| Agent CLI check | `agent-backend` healthcheck, 200 OK, p50 | StatusLED + MetaBadge |
| Kiosk lock | `enforcement-service`, kiosk · level 1000 | NodeCard (violet accent) |
| Escape phrase | BREAK GLASS circuit breaker | BreakGlass (dashed red) |
| Submit/commit actions | `▲ DEPLOY`, `SEND RESPONSE`, `ACK` | .cta.mono-cta |
| Streak | `uptime 17d` | MetaBadge teal |
| Carryover questions | `DLQ: n` (dead letter queue) | MetaBadge amber |
| Course timer | `TTL 27:14` | MetaBadge violet / TimerBar |
| Quiz score | `error budget` | MetaBadge |
| Quiz question | incoming request `POST /quiz/q2` | NodeCard request inspector |
| Background generation | `shard: generating` | StatusLED pending |
| Dashboard | cluster overview + request log | stat nodes + log table |

## Tokens (theme.css)

Two themes: **noir** (every screen) and **scholar** (course reader body only).
Topology chrome on top of noir:

- Blueprint grid: `radial-gradient(circle, var(--grid-dot) 1px, transparent 1px)` 22px.
- Node: bg `#1b1721`, border `#3a3344`, header divider `#2b2434`.
- LEDs: ok `#9fe1cb`, warn `#fac775`, err `#f09595`, pending pulses.
- Mono scale: 10px META_LABELS, 11px labels/badges, 13px values, 22px display digits.
- Serif (Fraunces) reserved for one editorial headline per screen.
- Pipes: 1.5px dashed SVG paths in node-accent colors; arrows optional.

## Components (src/lib/components/)

- `ClusterBar.svelte` — top status strip: `principia://<route> · cluster: <host>` left, status right.
- `NodeCard.svelte` — header (icon, name, badge) + body snippet; `accent` prop colors the border.
- `StatusLED.svelte` — `tone: ok|warn|err|idle|pending` (pending pulses).
- `MetaBadge.svelte` — mono pill, `tone: teal|amber|violet|red|muted`.
- `BreakGlass.svelte` — circuit-breaker escape hatch (replaces EscapeHatch visuals).
- `.cta.mono-cta` — deploy-style button (mono, uppercase, ▲ prefix where apt).
- `CourseGlyph.svelte` — nine small SVG course marks, using the existing amber
  strokes, violet connections and teal indicators; retain the original app mark.
- `Dropdown.svelte` — shared select-only combobox. Opaque node surface, violet
  active row, teal selection check, amber focus ring. Its menu escapes scrolling
  cards and flips above its trigger when space below is limited.

## Desktop controls

The user explicitly requested custom controls on 2026-09-09. Do not introduce
native `<select>` menus, browser time pickers, or `alert`/`confirm`/`prompt` dialogs.
Use the shared `Dropdown` and `TimePicker`; semantic buttons and inputs retain
keyboard and accessibility behavior with app-owned styling. Checkboxes and
radios use custom marks, and number inputs hide platform spinners.

Dropdowns support arrow keys, Home/End, type-ahead, Enter/Space confirmation,
Escape cancellation, Tab navigation, outside-click dismissal and disabled states.
Opening a dropdown explicitly focuses its trigger for WebKit. Menu positioning
must work within desktop windows, independently of enclosing cards or scroll panes.

The native window and noir surface explicitly use the dark theme. Set text color
on the app root, and keep navigation above scrolling content; do not rely on
system appearance or inherited browser defaults for contrast. The paper reader
keeps its light color scheme. The default desktop window is 1100 × 760, with a
640 × 540 minimum.

See [native verification](docs/DESKTOP_DESIGN_QA.md) for the build and test scope.

## Rules

- Sentence case prose; SCREAMING_SNAKE only for mono meta-labels (FIRE_AT, TTL).
- Every screen gets: blueprint grid + ClusterBar + one serif headline max.
- Metaphors must be honest — never label something with a concept it doesn't implement.
- Course reader body stays scholar (paper, 68ch, 1.8 line-height) — reading comfort wins;
  only its header bar speaks topology.

### Progress workspace

Progress uses the same contained workspace as Classes. A compact class list scopes the right pane; lifetime totals, a short activity strip and history filters provide an overview without a page-length report. History uses database pagination and its own keyboard-focusable scroll region, with fixed page controls. At narrow widths, a custom class dropdown replaces the sidebar. Short windows use a contained Overview/History switch. Archived reading occupies the detail area and returns to the preserved filters and focused lesson. Keep these controls within the shared radius and color tokens; do not restore the old stacked mastery/history report.

When the panel has enough height for both overview and history, align metric labels and values in a compact row and reduce decorative spacing. Give the recovered space to lesson rows. The short-panel Overview/History switch remains responsible for smaller windows and preparation banners.

The compact switch responds to the panel's available height, including space used by lesson-preparation status. It must not depend solely on window height: a normal-size window can still need the compact layout while another task is running. Native QA verified the summary/history switch and contained history controls with a real lesson preparing.
