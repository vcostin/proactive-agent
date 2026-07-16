# 03 — System prerequisites: detect and suggest

**What to build:** Setup Wizard / Setup repair surfaces structured system prerequisite results for the current OS and explains how to install what is missing. Detect + suggest is the model; the wizard does not elevate or run a general OS package manager. If a narrow exception (e.g. Windows VCRedist helper) remains temporarily, it is explicit — not silent policy drift. Platform-aware rows hide irrelevant checks.

**Blocked by:** None — can start immediately.

**Status:** done

- [x] System prerequisite check returns structured, platform-aware results usable by the UI
- [x] Missing prerequisites show clear guidance (what and how), not only a red flag
- [x] Default policy is detect + suggest; any install-helper exception is documented as such
- [x] Irrelevant OS-only rows are not shown as failures on other platforms
- [x] Tests cover reporting behaviour with mocked/fixture absence where practical
