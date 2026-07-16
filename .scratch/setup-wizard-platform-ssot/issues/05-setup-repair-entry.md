# 05 — Setup repair entry

**What to build:** After setup is complete, the user can explicitly reopen the Setup Wizard as Setup repair. Re-opening re-checks system prerequisites and app-managed artifact readiness. Memory and unrelated config are not wiped; memory reset remains a separate user action.

**Blocked by:** 01 — Core setup gate + status seam tests

**Status:** done

- [x] There is an explicit UI entry point to open Setup repair when the app is already ready
- [x] Opening Setup repair does not clear memory or unrelated config
- [x] Setup repair re-runs prerequisite and artifact checks on open
- [x] Forced re-entry when Core requirements are missing still works as a safety net
- [x] Wizard copy reflects setup/repair, not “first-run only”
