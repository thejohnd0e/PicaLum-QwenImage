# Technical Decisions

## NVIDIA auto-profile and verification plan

- Persist the selected GPU by stable NVIDIA UUID, not display ordinal. Telemetry and the ComfyUI child process must use that same UUID; never silently fall back to GPU 0.
- Use decimal `nvidia-smi` memory thresholds: below 10,000 MB unsupported; 10,000–11,999 MB recommends Compact; 12,000 MB or greater recommends Balanced. Current free VRAM is transient readiness information, not hardware eligibility.
- Eligibility requires NVIDIA/Windows, driver major 580+, the 10,000 MB floor, and a verified compatible compute capability. A GPU marketing/model name is not evidence of architecture compatibility.
- Capability is obtained from the installed bundled Python/PyTorch runtime after prerequisites exist, probed with the selected UUID in the child environment, comparing the runtime device UUID and the compiled `sm_XY` list (PTX is not trusted). Model-profile downloads do not begin until capability is proven; `resolve_setup` without proof returns `UnknownCapability`. Implemented and verified on an RTX 3060.
- Auto and Manual profile mode are distinct persisted states. Manual selection survives detection; Auto recomputes the shared recommendation.
- Existing configs remain loadable via serde defaults; config version was not changed for these new fields.
- Verification is a fixed Draft baseline, isolated from user output/history, bounded and cancellable. Skip/failure leaves the app usable; verification freshness is keyed by GPU UUID, profile, and engine fingerprint.
- External engines whose selected device cannot be proven must remain explicitly unverified.
- Do not add a CLI verification benchmark. CLI setup/doctor should share the same GPU support policy.

## Handoff state

The gate and verification are merged into `main` as build 0.1.10. See [`STATUS.md`](STATUS.md) for what is verified and what remains.
