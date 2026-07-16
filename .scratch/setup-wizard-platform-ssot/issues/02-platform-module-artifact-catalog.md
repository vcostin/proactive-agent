# 02 — Platform-module artifact catalog (expand)

**What to build:** Introduce Platform modules as the single source of truth for per-OS app-managed artifact definitions (names, fetch sources, destinations, verify/“ready?” rules). Ship a real Linux (Host OS) catalog plus Guest OS stubs that compile without executing Host-only install side effects. Existing wizard and Developer setup download paths keep working beside the new catalog (expand). Catalog verify behaviour is covered by tests.

**Blocked by:** None — can start immediately.

**Status:** done

- [x] Platform modules exist per supported OS with Linux definitions populated
- [x] Guest OS modules compile/register without breaking shared core or requiring Host-only side effects
- [x] A verify/“ready?” API answers from catalog definitions against fixture directories
- [x] Tests lock catalog verify behaviour for representative app-managed artifacts
- [x] Existing download paths still function (no forced cutover yet)
- [x] Vocabulary matches CONTEXT.md (Platform module, app-managed artifact, Host OS, Guest OS)
