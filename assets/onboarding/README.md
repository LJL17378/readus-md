# Readus.md first-launch animation

The active welcome screen is drawn by `src/onboarding.rs`: a pale blue ground, two flat five-book stacks with a small slow horizontal drift, and a short Readus.md wordmark introduction. It uses native quads, not a video texture, perspective, glow or grain. The blue is sampled from the current app icon. Low motion keeps travel to two points; High uses eight. The existing onboarding controls and order remain.

`intro.png` is retained as historical upstream Quiet.md artwork and is no longer loaded by the welcome screen. The original hillside animation was generated with Seedance 2.5 in LibTV, encoded as 400×225, 12fps XOR-delta indexed frames. Its encoder remains at `tools/onboarding/intro.py`; it is not part of the Readus.md welcome flow.
