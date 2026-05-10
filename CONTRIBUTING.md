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
git add -p          # stage selectively
git commit -m "feat: describe the change"

# 3. Push and open PR
git push -u origin feat/my-thing
gh pr create --fill

# 4. Merge (squash for noisy branches, merge commit for clean ones)
gh pr merge --squash    # or --merge

# 5. Clean up
git checkout master && git pull
git branch -d feat/my-thing
```

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

1. AI creates a feature branch: `feat/session-description`
2. All edits stay on that branch — **never edit main project files directly**
3. AI opens a PR when work is done; human reviews and merges
4. If the AI session tool creates a worktree, all edits go into that worktree's branch only

If a direct-to-master exception is taken (e.g. emergency fix mid-session), the AI must
add a comment in the PR or commit explaining why the normal flow was bypassed.
