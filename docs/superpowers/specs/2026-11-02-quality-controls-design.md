# Compact quality controls and negative-prompt removal

## Approved requirements

- Replace the `Quality` heading with `Quality presets`; make the preset list collapsible.
- Keep a compact summary of the selected preset/settings visible when collapsed.
- Below presets show manual Steps, Width and Height (pixel dimensions).
- Format remains the aspect-ratio selector; changing one dimension recalculates the other, rounded to multiples of 32.
- Minimum is 256 pixels **per axis**, not a minimum area or a claim about the model's supported minimum. Dimensions increment by 32 pixels.
- Selecting a preset fills manual values. Editing any manual value selects a visible `Custom` state; keep the underlying base preset for legacy fallback, never pass an unrecognized `custom` preset to core.
- Remove negative prompt only from the desktop interface, including storage reads, listeners and request values. Preserve CLI/core support. Saved negative text must not silently influence desktop generation.
- Record a future investigation into negative-prompt effectiveness; do not run that investigation or GPU experiments as part of this implementation.
- Preserve CFG, denoising, mask feathering, comparison and current output/history behavior.

## Approaches

1. Collapsible presets plus editable resolved parameters (chosen): compact, presets remain convenient, actual values are visible.
2. Separate mutually exclusive Preset/Manual tabs (not selected): adds another mode and hides useful values.
3. Independent presets and overrides with no Custom indication (rejected): ambiguous effective settings.

## Implementation decisions proposed by the planner

- Use native `details`/`summary`, collapsed initially; persist expansion separately from generation settings.
- Resolve preset values through shared core code, not a duplicate JavaScript preset table. Preserve all existing preset calculations, including Max's official aspect table and the separate photo edit resolution.
- On a Format change keep the current pixel-area budget, calculate the new dimensions, round to nearest 32 and scale up together if either axis falls below 256. Format is a nominal ratio: rounding means it is approximate; show the actual pixel dimensions.
- Reject invalid or non-multiple manually typed dimensions on validation rather than silently changing the submitted value. The automatically calculated companion dimension is rounded. If it would fall below 256, show an error and keep the last valid pair; do not silently change the axis the user just edited.
- In Surprise/Random, retain a size budget and resolve dimensions only after receiving that image's random aspect. Display that the dimensions vary, rather than a misleading fixed pair.

## Confirmed decisions

1. **Edit and Edit an area:** Steps are editable in all modes. Width/Height are editable in Create and fixed-format Surprise. Photo modes display effective source-derived dimensions read-only with an explanation. Explicit photo canvases are out of scope; do not ship enabled fields that are ignored.
2. **Application bounds:** Steps 1–100; each manually entered axis 256–4096 px; total area at most 4,500,000 pixels (includes current Max presets). These are application safety bounds, not official model limits. Do not apply new manual limits retroactively to legacy source-derived photo canvases.

## Constraints and delivery

- Work in the existing plain JavaScript/Tauri/Rust architecture; shared resolution and validation belong in `qil-core`.
- Backward-compatible optional request fields; requests without overrides retain the old behavior.
- English, Russian and Spanish UI strings; keyboard operation, labels, focus and busy-state locking.
- Do not touch user-owned `.agents/` or `skills-lock.json`.
- Before producing a new build increment the current version by exactly one in all manifests/locks. At planning time the version is 0.1.24; next build is 0.1.25 only if no intervening build occurred.
- This planning task makes no product changes, produces no build, and requires no version bump.
