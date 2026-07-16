# 08 — Contract duplicate artifact lists

**What to build:** Once both the Setup Wizard and Developer setup consume Platform modules, remove the old duplicated artifact URL/name/destination lists so the catalog is the only source of truth (contract step of expand–contract).

**Blocked by:** 04 — Setup Wizard consumes Platform modules; 07 — Developer setup consumes Platform modules

**Status:** done

- [x] Duplicated artifact definitions outside Platform modules are removed or reduced to thin wrappers
- [x] Wizard and Developer setup still succeed on Host OS after the contract
- [x] Catalog tests (or equivalent) remain green as the sole definition of artifact metadata
- [x] No behaviour regressions to Core agent or Host STT on Linux when artifacts are present
