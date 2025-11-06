# NixOS Packaging Guide for http-nu

## Overview

This guide explains how to package http-nu as a NixOS package and maintain it in nixpkgs. http-nu is a Rust application, so we'll use `buildRustPackage` from nixpkgs.

## Why Package for NixOS?

- **Reproducibility**: Users get exact versions with all dependencies
- **Distribution**: Easy installation via `nix-env -iA nixpkgs.http-nu` or in NixOS configuration
- **Maintenance**: Automated updates via nixpkgs bots and community help
- **Testing**: CI automatically tests on multiple platforms

## Understanding the NixOS Packaging Process

### 1. Package Definition Structure

NixOS packages are defined using the Nix expression language. For Rust applications, the basic structure is:

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
    hash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
  };

  cargoHash = "sha256-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB=";

  meta = {
    description = "Serve a Nushell closure over HTTP";
    homepage = "https://github.com/cablehead/http-nu";
    license = lib.licenses.mit;
    maintainers = with lib.maintainers; [ ]; # Add maintainer handles
    mainProgram = "http-nu";
    platforms = lib.platforms.all;
  };
}
```

### 2. Key Components Explained

#### a. Function Parameters
```nix
{
  lib,              # nixpkgs library utilities
  rustPlatform,     # provides buildRustPackage
  fetchFromGitHub,  # helper to fetch from GitHub
}:
```

These are dependencies that nixpkgs provides automatically.

#### b. Source Fetching
```nix
src = fetchFromGitHub {
  owner = "cablehead";
  repo = "http-nu";
  rev = "v${version}";  # Git tag or commit
  hash = "sha256-...";   # Hash of the source tarball
};
```

This fetches the source code from GitHub. The hash ensures reproducibility.

#### c. Cargo Hash
```nix
cargoHash = "sha256-...";
```

This is a hash of all Cargo dependencies. It ensures that the exact same dependencies are used every time.

#### d. Metadata
```nix
meta = {
  description = "...";  # Brief description
  homepage = "...";      # Project website
  license = ...;         # License identifier
  maintainers = ...;     # Who maintains this package
  mainProgram = "...";   # Main executable name
  platforms = ...;       # Supported platforms
};
```

## Step-by-Step Packaging Process

### Step 1: Create Initial Package Definition

**a. Fork nixpkgs repository**
```bash
# On GitHub, fork https://github.com/NixOS/nixpkgs
# Then clone your fork
git clone https://github.com/YOUR_USERNAME/nixpkgs.git
cd nixpkgs
git remote add upstream https://github.com/NixOS/nixpkgs.git
```

**b. Create a new branch**
```bash
git switch --create http-nu upstream/master
```

**c. Create package directory**

Modern nixpkgs uses the `pkgs/by-name/` convention:
```bash
mkdir -p pkgs/by-name/ht/http-nu
```

The structure is: `pkgs/by-name/<first-two-letters>/<package-name>/package.nix`

**d. Create `package.nix`**

Create `pkgs/by-name/ht/http-nu/package.nix`:

```nix
{
  lib,
  rustPlatform,
  fetchFromGitHub,
  nushell,
}:

rustPlatform.buildRustPackage rec {
  pname = "http-nu";
  version = "0.5.0";

  src = fetchFromGitHub {
    owner = "cablehead";
    repo = "http-nu";
    rev = "v${version}";
    hash = lib.fakeHash;  # Temporary - we'll get the real hash
  };

  cargoHash = lib.fakeHash;  # Temporary - we'll get the real hash

  meta = {
    description = "Serve a Nushell closure over HTTP";
    homepage = "https://github.com/cablehead/http-nu";
    changelog = "https://github.com/cablehead/http-nu/blob/v${version}/changes/v${version}.md";
    license = lib.licenses.mit;
    maintainers = with lib.maintainers; [ ];
    mainProgram = "http-nu";
    platforms = lib.platforms.unix;  # Adjust based on actual support
  };
}
```

### Step 2: Get the Correct Hashes

**a. Get the source hash**

Build the package with the fake hash to get the correct one:
```bash
nix-build -A http-nu
```

You'll see an error like:
```
error: hash mismatch in fixed-output derivation
  specified: sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
     got:    sha256-abc123def456...
```

Copy the "got" hash and replace `lib.fakeHash` in the `src` section.

**b. Get the cargo hash**

After fixing the source hash, build again:
```bash
nix-build -A http-nu
```

You'll see another hash mismatch for `cargoHash`. Copy that hash and replace `lib.fakeHash` in the `cargoHash` field.

**c. Final build**

Build once more to verify:
```bash
nix-build -A http-nu
```

If successful, you'll see `./result` symlink pointing to the built package.

### Step 3: Test the Package

**a. Test the binary**
```bash
./result/bin/http-nu --version
./result/bin/http-nu --help
```

**b. Test in a nix-shell**
```bash
nix-shell -p '(import ./. {}).http-nu'
# Inside the shell:
which http-nu
http-nu --version
```

**c. Use nixpkgs-review (recommended)**
```bash
# Install nixpkgs-review if you don't have it
nix-env -iA nixpkgs.nixpkgs-review

# Review your changes
nixpkgs-review wip
```

This will build your package and all its reverse dependencies to ensure nothing breaks.

### Step 4: Submit to nixpkgs

**a. Commit your changes**
```bash
git add pkgs/by-name/ht/http-nu/package.nix
git commit -m "http-nu: init at 0.5.0"
```

Follow nixpkgs commit conventions:
- Format: `package-name: action`
- Actions: `init at X.Y.Z` (new package), `X.Y.Z -> A.B.C` (update), `fix build`, etc.

**b. Push to your fork**
```bash
git push --set-upstream origin http-nu
```

**c. Create Pull Request**

Go to https://github.com/YOUR_USERNAME/nixpkgs and create a PR to NixOS/nixpkgs.

Use this template:

```markdown
## Description

Adds http-nu, a tool to serve Nushell closures over HTTP.

## Testing

- [x] Built on x86_64-linux
- [x] Tested `http-nu --version`
- [x] Tested `http-nu --help`
- [x] Ran basic HTTP server test
- [x] Ran nixpkgs-review

## Additional Notes

- First release in nixpkgs
- Upstream project: https://github.com/cablehead/http-nu
- License: MIT
```

**d. Wait for review**

- The ofborg bot will automatically test your package on multiple platforms
- Maintainers will review your PR (this can take days to weeks)
- Respond to any feedback and update your PR as needed

### Step 5: Respond to Review Feedback

**Important lesson from http-nu PR**: Reviewers discourage creating "orphan packages" (packages with no maintainers). If you care enough to package something, you should become its maintainer.

#### Adding Yourself as Maintainer

If reviewers ask you to add yourself as maintainer, follow these steps:

**a. Get your GitHub ID**
```bash
curl -s https://api.github.com/users/YOUR_USERNAME | jq .id
```

**b. Add yourself to maintainer-list.nix**

Find your alphabetical position and add your entry:
```nix
yourusername = {
  email = "your@email.com";
  github = "yourusername";
  githubId = 12345678;
  name = "Your Name";
};
```

**c. Update the package to list you as maintainer**
```nix
maintainers = with lib.maintainers; [ yourusername ];
```

**d. Organize commits properly**

Reviewers prefer this commit structure:
1. `package-name: init at X.Y.Z` (includes formatting fixes)
2. `maintainers: add username` (or `maintainers: add user1 and user2`)
3. `package-name: add maintainer username`

**e. Rewrite commit history**

If you need to reorganize commits:
```bash
# Soft reset to combine commits
git reset --soft HEAD~2

# Create properly organized commits
git commit -m "http-nu: init at 0.5.0"
git add maintainers/maintainer-list.nix
git commit -m "maintainers: add yourusername"
git add pkgs/by-name/ht/http-nu/package.nix
git commit -m "http-nu: add maintainer yourusername"
```

**f. Force push safely**
```bash
# Use --force-with-lease (safer than --force)
git push --force-with-lease origin your-branch
```

The `--force-with-lease` option will fail if someone else pushed to your branch, preventing accidental overwrites.

**Tip**: Consider adding the upstream package author as a co-maintainer. They'll appreciate being able to maintain their package in nixpkgs!

### Step 6: Get Merged

Once approved, a maintainer will merge your PR, or you can request:
```
@NixOS/nixpkgs-merge-bot merge
```

## Maintaining the Package

### Regular Updates

When a new version is released, update the package:

**a. Create update branch**
```bash
cd nixpkgs
git fetch upstream
git switch --create update-http-nu upstream/master
```

**b. Update `package.nix`**

Change the version:
```nix
version = "0.6.0";  # Update this
```

**c. Update hashes**

Use `nix-build` to get new hashes (same process as initial packaging):
```bash
# Replace source hash with lib.fakeHash temporarily
nix-build -A http-nu  # Get source hash
# Update source hash
nix-build -A http-nu  # Get cargo hash
# Update cargo hash
nix-build -A http-nu  # Verify it builds
```

**d. Submit update PR**
```bash
git add pkgs/by-name/ht/http-nu/package.nix
git commit -m "http-nu: 0.5.0 -> 0.6.0"
git push --set-upstream origin update-http-nu
```

### Automated Updates

Consider using:
- **nixpkgs-update bot**: Automatically opens PRs for updates
- **r-ryantm bot**: Same as above, the community bot that handles many updates
- You can add `passthru.updateScript` to enable automatic updates:

```nix
passthru.updateScript = nix-update-script { };
```

### Becoming a Maintainer

Add yourself to the maintainers list:

**a. Add your handle to nixpkgs maintainers**

Edit `maintainers/maintainer-list.nix` and add yourself:
```nix
yourhandle = {
  email = "you@example.com";
  name = "Your Name";
  github = "yourgithub";
  githubId = 123456;
};
```

**b. Add yourself to http-nu**
```nix
maintainers = with lib.maintainers; [ yourhandle ];
```

This ensures you're notified of issues and PRs related to http-nu.

## Tips and Best Practices

### Use nix-update for easier updates

Install nix-update:
```bash
nix-env -iA nixpkgs.nix-update
```

Update packages easily:
```bash
nix-update http-nu --version 0.6.0
```

This automatically updates version and hashes!

### Add tests

Consider adding integration tests:
```nix
passthru.tests = {
  version = testers.testVersion { package = http-nu; };
};
```

### Handle dependencies

If http-nu needs system libraries, add them:
```nix
buildInputs = [ openssl ];  # Runtime dependencies
nativeBuildInputs = [ pkg-config ];  # Build-time dependencies
```

### Cross-compilation

For better cross-platform support, test:
```bash
nix-build -A pkgsCross.aarch64-multiplatform.http-nu
```

## Lessons Learned from http-nu Packaging

When packaging http-nu v0.5.0 for nixpkgs, we encountered the following (real experiences from PR #458947):

### Tests Failing in Sandbox

The Cargo tests failed in the Nix sandbox because they:
- Try to access file system paths not available in the sandbox
- Attempt network operations
- Need resources not provided during build

**Solution**: We added `doCheck = false;` to the package definition:

```nix
cargoHash = "sha256-...";

# Tests fail in sandbox due to file system and network access requirements
doCheck = false;

meta = {
```

This is a **common and accepted practice** in nixpkgs. Many packages disable tests when they can't run in the sandbox. The important thing is that the binary still builds correctly and works when tested manually (which http-nu does - version and help commands work perfectly).

### Version and Git Tag Convention

http-nu follows the standard convention of using `v{version}` for git tags:
- Cargo.toml shows `version = "0.5.0"`
- Git tag is `v0.5.0`
- In package.nix: `rev = "v${version}";` expands to `"v0.5.0"`

This is the most common pattern in Rust projects.

### Treefmt Formatting

The nixpkgs CI includes a `treefmt` check that enforces consistent formatting. When we submitted the initial PR, it failed because:

```nix
# Before (incorrect):
maintainers = with lib.maintainers; [ ];

# After (correct):
maintainers = [ ];
```

When the maintainers list is empty, the `with lib.maintainers;` scope is unnecessary and should be removed.

**How to fix formatting issues:**

Run treefmt in the nixpkgs repository:
```bash
cd ~/nixpkgs
nix-shell --run treefmt
# Or: nix develop --command treefmt
# Or: nix fmt
```

This will automatically format all changed files according to nixpkgs standards. Always run this before committing!

## Common Issues

### Issue: "hash mismatch"
**Solution**: Always get the correct hash by building with `lib.fakeHash` first.

### Issue: "cargo dependency not found"
**Solution**: Make sure `Cargo.lock` is committed in the upstream repo. If not, you might need to use `cargoLock.lockFile` with a generated lock file.

### Issue: "tests fail during build"
**Solution**: You can skip tests if they don't work in sandbox:
```nix
doCheck = false;
```

But investigate why they fail and fix if possible.

### Issue: "package not building on Darwin (macOS)"
**Solution**: May need Darwin-specific dependencies:
```nix
buildInputs = lib.optionals stdenv.hostPlatform.isDarwin [
  darwin.apple_sdk.frameworks.Security
];
```

## Resources

- **Official nixpkgs manual**: https://nixos.org/manual/nixpkgs/stable/
- **Rust section**: https://ryantm.github.io/nixpkgs/languages-frameworks/rust/
- **Contributing guide**: https://github.com/NixOS/nixpkgs/blob/master/CONTRIBUTING.md
- **Search existing packages**: https://search.nixos.org/packages
- **NixOS Wiki**: https://nixos.wiki/wiki/Rust
- **Package status**: https://nixpk.gs/pr-tracker.html

## Automation Workflow

For maintainers who want to automate releases:

### GitHub Actions Integration

You could set up a GitHub Action in the http-nu repo that:
1. Detects new tags
2. Opens a PR to nixpkgs with updated version
3. Uses nix-update to handle hash updates

Example workflow concept:
```yaml
name: Update nixpkgs
on:
  release:
    types: [published]
jobs:
  update-nix:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout nixpkgs fork
        # ... fork nixpkgs, create branch
      - name: Update package
        run: nix-update http-nu --version ${{ github.event.release.tag_name }}
      - name: Create PR
        # ... create PR to NixOS/nixpkgs
```

This ensures updates happen automatically when you release!

## Questions?

- **NixOS Discourse**: https://discourse.nixos.org/
- **Matrix chat**: #nix:nixos.org
- **GitHub Discussions**: https://github.com/NixOS/nixpkgs/discussions

Feel free to ask for help - the community is very welcoming to new contributors!
