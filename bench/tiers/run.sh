#!/usr/bin/env bash
# VRAM tiers: real 12 GB vs an emulated 8 GB card (ComfyUI --reserve-vram hides 4.3 GB).
# Usage: bash bench/tiers/run.sh   (from the repo root, runtime installed, compact weights downloaded)
set -u
QIL=${QIL:-./target/release/qil.exe}
run() { # name profile engine_args
  local out="bench/tiers/results/$1"
  "$QIL" config --profile "$2" >/dev/null
  QIL_ENGINE_ARGS="$3" "$QIL" bench --suite bench/tiers/suite.json --inputs bench/inputs --out "$out" --only t2i_draft --preset draft --force
  QIL_ENGINE_ARGS="$3" "$QIL" bench --suite bench/tiers/suite.json --inputs bench/inputs --out "$out" --only t2i_standard_16x9 --only edit_snow --preset standard --force
}
run 12gb-balanced balanced ""
run 8gb-compact compact "--reserve-vram 4.3"
run 8gb-balanced balanced "--reserve-vram 4.3"
"$QIL" config --profile balanced >/dev/null
