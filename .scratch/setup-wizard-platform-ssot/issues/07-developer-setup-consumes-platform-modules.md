# 07 — Developer setup consumes Platform modules

**What to build:** Developer setup (`deno task setup` and related fetch scripts) remains a separate orchestrator but consumes the same Platform-module artifact contract for names, sources, destinations, and verify rules — so CLI and wizard stop drifting.

**Blocked by:** 02 — Platform-module artifact catalog (expand)

**Status:** done

- [x] Developer setup resolves Host OS artifacts from the Platform-module contract (or a generated projection of it)
- [x] Developer setup and Setup Wizard agree on “ready?” for the same fixture layout
- [x] CLI remains usable for developers without going through the Shipped-app UI
- [x] Scripts no longer own a divergent parallel URL/name list for artifacts moved into the catalog
- [x] Guest OS script entry points stay non-crashing / clearly unsupported where not yet populated
