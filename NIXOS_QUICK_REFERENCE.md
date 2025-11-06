# NixOS Packaging Quick Reference

Quick command reference for packaging and maintaining http-nu in nixpkgs.

## Initial Setup (One Time)

```bash
# Fork nixpkgs on GitHub, then:
git clone https://github.com/YOUR_USERNAME/nixpkgs.git
cd nixpkgs
git remote add upstream https://github.com/NixOS/nixpkgs.git
```

## Creating a New Package

```bash
# 1. Create branch
git fetch upstream
git switch --create http-nu upstream/master

# 2. Create package directory
mkdir -p pkgs/by-name/ht/http-nu

# 3. Create package.nix
# (Copy from package.nix.example and edit)

# 4. Get source hash
nix-build -A http-nu
# Copy the "got" hash to src.hash

# 5. Get cargo hash
nix-build -A http-nu
# Copy the "got" hash to cargoHash

# 6. Verify build
nix-build -A http-nu

# 7. Test
./result/bin/http-nu --version

# 8. Review (optional but recommended)
nixpkgs-review wip

# 9. Commit
git add pkgs/by-name/ht/http-nu/package.nix
git commit -m "http-nu: init at 0.5.0"

# 10. Push and create PR
git push --set-upstream origin http-nu
# Then create PR on GitHub
```

## Updating Existing Package

```bash
# 1. Create update branch
git fetch upstream
git switch --create update-http-nu upstream/master

# 2. Update package.nix
# Change version number

# 3. Get new hashes (same as initial setup)
# Set src.hash to lib.fakeHash
nix-build -A http-nu  # Get source hash
# Update src.hash
nix-build -A http-nu  # Get cargo hash
# Update cargoHash
nix-build -A http-nu  # Verify

# 4. Test
./result/bin/http-nu --version

# 5. Commit and push
git add pkgs/by-name/ht/http-nu/package.nix
git commit -m "http-nu: 0.5.0 -> 0.6.0"
git push --set-upstream origin update-http-nu
# Create PR on GitHub
```

## Using nix-update (Easier Updates)

```bash
# Install nix-update
nix-env -iA nixpkgs.nix-update

# Update package automatically
cd nixpkgs
git switch --create update-http-nu upstream/master
nix-update http-nu --version 0.6.0
git add pkgs/by-name/ht/http-nu/package.nix
git commit -m "http-nu: 0.5.0 -> 0.6.0"
git push --set-upstream origin update-http-nu
```

## Testing Commands

```bash
# Build the package
nix-build -A http-nu

# Test the binary
./result/bin/http-nu --version
./result/bin/http-nu --help

# Test in shell
nix-shell -p '(import ./. {}).http-nu'

# Review all changes
nixpkgs-review wip

# Cross-compile test
nix-build -A pkgsCross.aarch64-multiplatform.http-nu
```

## Useful Git Commands

```bash
# Sync with upstream
git fetch upstream
git rebase upstream/master

# Amend last commit
git commit --amend

# Force push (after rebase)
git push --force-with-lease

# Check your branch status
git status
git log --oneline -5
```

## PR Commit Message Format

### New Package
```
http-nu: init at 0.5.0
```

### Update
```
http-nu: 0.5.0 -> 0.6.0
```

### Fix
```
http-nu: fix build on darwin
```

### Refactor
```
http-nu: refactor derivation
```

## Getting Help

- **Build issues**: Check `nix-build` output carefully
- **Hash mismatches**: Use `lib.fakeHash` to get correct hash
- **CI failures**: Check ofborg comments on PR
- **Questions**: Ask on https://discourse.nixos.org/

## Hash Conversion

If you have an old-style hash:
```bash
nix-hash --to-sri --type sha256 "<old-hash>"
```

## Finding Maintainer ID

Look up your GitHub ID:
```bash
curl -s https://api.github.com/users/YOUR_GITHUB_USERNAME | jq .id
```

## Package.nix Template

```nix
{
  lib,
  rustPlatform,
  fetchFromGitHub,
}:

rustPlatform.buildRustPackage rec {
  pname = "http-nu";
  version = "0.5.0";

  src = fetchFromGitHub {
    owner = "cablehead";
    repo = "http-nu";
    rev = "v${version}";
    hash = "sha256-...";
  };

  cargoHash = "sha256-...";

  meta = {
    description = "Serve a Nushell closure over HTTP";
    homepage = "https://github.com/cablehead/http-nu";
    changelog = "https://github.com/cablehead/http-nu/blob/v${version}/changes/v${version}.md";
    license = lib.licenses.mit;
    maintainers = with lib.maintainers; [ yourhandle ];
    mainProgram = "http-nu";
  };
}
```

## Adding Yourself as Maintainer

```bash
# Get your GitHub ID
curl -s https://api.github.com/users/YOUR_USERNAME | jq .id

# Add to maintainer-list.nix (find alphabetical position)
# yourusername = {
#   email = "your@email.com";
#   github = "yourusername";
#   githubId = 12345678;
#   name = "Your Name";
# };

# Update package.nix
# maintainers = with lib.maintainers; [ yourusername ];
```

## Reorganizing Commits

```bash
# If reviewers ask for different commit structure
git reset --soft HEAD~2  # Combine last 2 commits

# Create 3 separate commits
git commit -m "package-name: init at X.Y.Z"
git add maintainers/maintainer-list.nix
git commit -m "maintainers: add username"
git add pkgs/by-name/pk/package-name/package.nix
git commit -m "package-name: add maintainer username"

# Force push safely
git push --force-with-lease origin branch-name
```

## Checklist for New Package

- [ ] Package builds successfully
- [ ] Binary runs and shows correct version
- [ ] Tests pass (if enabled)
- [ ] Metadata is accurate
- [ ] License is correct
- [ ] Maintainer is added (you!)
- [ ] Added yourself to maintainer-list.nix
- [ ] Changelog link works
- [ ] PR description is complete
- [ ] nixpkgs-review passes

## Checklist for Updates

- [ ] Version bumped correctly
- [ ] Both hashes updated
- [ ] Package builds
- [ ] Binary works
- [ ] Changelog link updated
- [ ] Commit message follows format
- [ ] No unrelated changes included
