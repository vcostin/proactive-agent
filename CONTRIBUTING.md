# Contributing & Git Workflow

## Branch convention

```
master          ← stable, always deployable
  │
  ├── feat/tts-espeak-data     ← new feature
  ├── fix/installer-dll-gap    ← bug fix
  ├── chore/remove-kokoro      ← cleanup / deps
  ├── docs/architecture-rewrite
  └── security/csp-hardening
```

**Every change** — feature, fix, refactor, docs — goes on a branch.
Direct commits to master are not allowed except for single-line typo fixes.

## Workflow

```bash
# 1. Create branch from current master
git checkout master && git pull
git checkout -b feat/my-thing

# 2. Work, commit often
git add -p
git commit -m "feat: describe the change"

# 3. Merge locally and push
git checkout master
git merge --no-ff feat/my-thing   # --no-ff preserves branch in history
git push

# 4. Clean up
git branch -d feat/my-thing
```

> **Note:** PRs are skipped for now — this is a small solo project and the overhead
> isn't justified yet. Switch to the full PR review cycle when the architecture
> stabilises (primary trigger: Python dependency removed from the stack).
> The branch discipline stays the same regardless.

## Commit message format

```
<type>: <short description>

[optional body — explain WHY, not WHAT]
```

| Type | When |
|------|------|
| `feat` | New capability |
| `fix` | Bug fix |
| `security` | Security hardening |
| `chore` | Dependencies, cleanup, tooling |
| `docs` | Documentation only |
| `test` | Tests only |
| `refactor` | Code change with no behaviour change |

## When a direct-to-master exception is allowed

Only two cases — both must be noted in the commit message:

1. **Dependency lock updates** — `cargo update` / `npm update` with no API changes
2. **Emergency hotfix** — security vulnerability in production, no time for PR cycle;
   must be followed immediately by a retro commit explaining why

Example exception note:
```
chore: bump reqwest 0.12.27 → 0.12.28 (security patch CVE-XXXX-YYYY)

EXCEPTION: direct-to-master — CVE patch with no API changes, low risk.
```

## AI session workflow

When an AI assistant (Claude, etc.) works on this repo:

1. AI creates a feature branch at session start: `feat/session-description`
2. All edits stay on that branch — **never edit the main working tree directly**
3. AI merges to master locally and pushes when work is complete
4. If the AI session tool creates a worktree, all edits go into that worktree's branch only

If a direct-to-master exception is taken, the AI must note it in the commit message with
a brief reason — not for process's sake, but so the history is readable.
