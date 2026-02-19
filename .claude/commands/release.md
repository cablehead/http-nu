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
- **No soft line breaks** -- paragraphs should be single long lines, not wrapped at 80 columns. GitHub renders markdown with soft wraps, so hard breaks mid-paragraph show up as unwanted newlines in the release notes.

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

### 7. Homebrew Formula Update

- Clone `../homebrew-tap` if not present:
  `git clone https://github.com/cablehead/homebrew-tap.git`
- **Pull latest** before making changes: `cd ../homebrew-tap && git pull`
- **Wait 10+ seconds** after build completes for GitHub CDN propagation
- Download macOS tarball, verify integrity, and calculate SHA256:
  ```bash
  cd /tmp
  rm -f http-nu-v$ARGUMENTS-darwin-arm64.tar.gz
  curl -sL https://github.com/cablehead/http-nu/releases/download/v$ARGUMENTS/http-nu-v$ARGUMENTS-darwin-arm64.tar.gz -o http-nu-v$ARGUMENTS-darwin-arm64.tar.gz
  # Verify download by extracting and checking binary
  tar -tzf http-nu-v$ARGUMENTS-darwin-arm64.tar.gz  # should list: http-nu-v$ARGUMENTS/http-nu
  sha256sum http-nu-v$ARGUMENTS-darwin-arm64.tar.gz
  ```
- Update `../homebrew-tap/Formula/http-nu.rb` with new version, URL, and SHA256
  checksum
- Commit and push homebrew formula changes

### 8. Manual Verification Required

**⚠️ CRITICAL: macOS Verification BEFORE Publishing to Crates.io**

After homebrew formula is updated, **PAUSE** and ask a macOS user to test:

```bash
brew uninstall http-nu  # if previously installed
brew install cablehead/tap/http-nu
http-nu --version  # should show v$ARGUMENTS
```

**STOP HERE if verification fails.** Publishing to crates.io is irreversible.

### 9. Cargo Publish

**Only proceed after macOS verification passes.**

```bash
cargo publish
```

**Warning**: This step cannot be undone - you cannot unpublish from crates.io

### 10. Bump to Dev Version

After publishing, bump `Cargo.toml` to the next patch dev version (e.g., `0.12.0` -> `0.12.1-dev`), run `cargo check` to update `Cargo.lock`, and commit:

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: bump to v<next>-dev"
git push
```

## Release Complete

- GitHub release: https://github.com/cablehead/http-nu/releases/tag/v$ARGUMENTS
- Homebrew: `brew install cablehead/tap/http-nu`
- Crates.io: `cargo install http-nu`
