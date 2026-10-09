# PicaLum QwenImage Design System

## 1. Atmosphere & Identity

The desktop interface is a compact, keyboard-friendly darkroom: graphite surfaces, restrained dividers, dense controls, and state communicated through text and tonal changes. Preserve the existing application character; this change adds a prompt library without restyling the shell.

## 2. Color

| Role | Existing token | Value | Usage |
|---|---|---|---|
| Main background | `--bg` | `#191e23` | Viewer and app canvas |
| Panel surface | `--panel` | `#20262d` | Tool panel, controls, dialogs |
| Sunken surface | `--sunken` | `#171b20` | Inputs, selected surfaces |
| Primary text | `--text` | `#eef2f5` | Main content |
| Muted text | `--text-muted` | `#a2acb6` | Labels and hints |
| Faint text | `--text-faint` | `#77838e` | Placeholder and subdued metadata |
| Divider | `--border` | `#303942` | Borders and separators |
| Strong divider | `--border-strong` | `#424d58` | Hover and emphasis |
| Hover surface | `--hover` | `#282f37` | Interactive hover fill |
| Primary action | `--primary` | `#c2d6ef` | Generate action and key state |
| Primary hover | `--primary-hover` | `#d3e1f2` | Primary action hover |
| Success / warning / error | `--ok` / `--pend` / `--err` | Existing CSS tokens | Status feedback |

Extend or reuse CSS custom properties; do not add unrelated palette values.

## 3. Typography

- Sans: `--font-sans` (`Segoe UI Variable Text`, `Segoe UI`, system fallbacks).
- Mono: `--font-mono` (`Cascadia Mono`, `Cascadia Code`, Consolas, monospace).
- Existing scale is intentionally compact: 10–15px for metadata, controls, and labels; body defaults to 13px. Keep the surrounding density and existing size conventions.

## 4. Spacing & Layout

- Existing spacing follows a compact 2/4/6/8/10/12/16/20/22/24/28/32px rhythm.
- The desktop shell is a bounded two-column grid. The tool panel owns vertical scrolling; its Generate footer stays fixed within the panel. The stage owns the viewer and history on the opposite side.
- Breakpoints: 1040px and 680px, matching `styles.css`. Preserve the current minimum-window behavior (1060×680).
- Modal editor body owns scrolling when content exceeds the window; overlay and title/actions remain available.

## 5. Components

### Tool panel and stage
- **Structure:** `.app` grid with `.panel` and `.stage`.
- **States:** mode-specific groups use `hidden`; panel content scrolls independently; Generate footer remains fixed.
- **Accessibility:** mode tabs and controls retain their existing semantic labels and keyboard behavior.

### Compact controls and groups
- **Structure:** `.group`, `.label`, `.hint`, `.btn`, text fields.
- **States:** default, hover, focus-visible, disabled, and error follow existing tokens and button/input rules.
- **Accessibility:** native controls, visible keyboard focus, associated labels or accessible names.

### Preset editor dialog
- **Structure:** native form controls inside a modal dialog, with a list and editable detail fields.
- **States:** closed, open, selected, empty, loading, saving, and error. Selection uses the existing sunken tonal surface; keyboard focus uses the established focus-visible treatment.
- **Accessibility:** `role="dialog"`, `aria-modal`, labelled heading, keyboard focus containment, Escape cancellation, and an announced error region.
- **Layout:** list and form are side by side when space permits and stack at narrow widths; dialog content scrolls independently.

## 6. Motion & Interaction

- Existing control feedback uses short CSS transitions (about 120ms) and the app honors `prefers-reduced-motion`.
- New editor interactions remain immediate and state-led; do not add decorative motion. Focus returns to the Manage control when the dialog closes.

## 7. Depth & Surface

- Strategy: mixed, using thin tokenized borders for separation and restrained existing shadows for overlays.
- Radius tokens: `--r` (3px) and `--r-sm` (2px). Match neighboring controls rather than introducing pill-shaped surfaces.

## 8. Accessibility Constraints & Accepted Debt

### Constraints
- Every form control has an associated label or accessible name. Interactive controls remain keyboard reachable with visible focus. Dialog focus is contained and restored; reduced-motion preferences are respected.

### Accepted Debt
| Item | Location | Why accepted | Owner / Exit |
|---|---|---|---|
| Compact typography below 14px | Existing desktop UI | Established information density and user-selected style; changing it is outside this feature | Revisit only in an approved visual redesign |
