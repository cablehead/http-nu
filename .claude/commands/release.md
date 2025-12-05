---
allowed-tools: Bash, Edit, Read, Glob
argument-hint: [version] (e.g., 0.6.0)
description: Automated release process - version bump, changelog, tag, publish
---

# Release Process for http-nu

## Pre-flight Checks

Current branch: !`git branch --show-current`

Last releases: !`git tag --sort=-version:refname | grep -v dev | head -5`

Current version: !`grep '^version' Cargo.toml | head -1`

## Steps

### 1. Version Bump

- Update version in `Cargo.toml` to $ARGUMENTS
- Run `cargo check` to update `Cargo.lock`

### 2. Generate Changelog

Get commits since last stable release:

```bash
last_tag=$(git tag --sort=-version:refname | grep -v dev | head -1)
git log --oneline --pretty=format:"* %s (%ad)" --date=short ${last_tag}..HEAD
```

Create `changes/v$ARGUMENTS.md` with:
- `# v$ARGUMENTS` header
- `## Highlights` section with notable user-facing changes
- `## Raw commits` section with commit list

### 3. Review

**⚠️ REVIEW REQUIRED**: Show the changelog for user approval before proceeding.

### 4. Commit and Tag

```bash
git add Cargo.toml Cargo.lock changes/v$ARGUMENTS.md
git commit -m "chore: release v$ARGUMENTS"
git tag v$ARGUMENTS
```

### 5. Push

```bash
git push && git push --tags
```

This triggers the GitHub workflow to build cross-platform binaries.

### 6. Monitor Build

```bash
gh run list --limit 1
gh run watch <run-id> --exit-status
```

### 7. Cargo Publish (Optional)

After CI completes and binaries are verified:

```bash
cargo publish
```

## Release Complete

- GitHub release: https://github.com/cablehead/http-nu/releases/tag/v$ARGUMENTS
- Crates.io: `cargo install http-nu`
