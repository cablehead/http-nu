# NixOS Packaging Guide for http-nu

This guide documents how to package and maintain http-nu in nixpkgs.

> **Note**: http-nu is already packaged in nixpkgs. See the
> [package definition](https://github.com/NixOS/nixpkgs/blob/master/pkgs/by-name/ht/http-nu/package.nix)
> for the current implementation.

**Reference PRs:**
- [#458947](https://github.com/NixOS/nixpkgs/pull/458947) - Initial package submission (v0.5.0)
- [#478224](https://github.com/NixOS/nixpkgs/pull/478224) - Version update (0.5.0 -> 0.9.0)

## Package Structure

Rust packages use `buildRustPackage`. Location: `pkgs/by-name/ht/http-nu/package.nix`

```nix
{
  lib,
  rustPlatform,
  fetchFromGitHub,
  stdenvNoCC,
  versionCheckHook,
  nix-update-script,
}:

rustPlatform.buildRustPackage (finalAttrs: {
  pname = "http-nu";
  version = "0.9.0";

  src = fetchFromGitHub {
    owner = "cablehead";
    repo = "http-nu";
    tag = "v${finalAttrs.version}";
    hash = "sha256-...";
  };

  cargoHash = "sha256-...";

  # Darwin needs bindgenHook for libproc
  nativeBuildInputs = lib.optionals stdenvNoCC.hostPlatform.isDarwin [
    rustPlatform.bindgenHook
  ];

  # Tests require network/filesystem access unavailable in sandbox
  doCheck = false;

  doInstallCheck = true;
  nativeInstallCheckInputs = [ versionCheckHook ];
  versionCheckProgramArg = "--version";

  passthru.updateScript = nix-update-script { };

  meta = {
    description = "Serve a Nushell closure over HTTP";
    homepage = "https://github.com/cablehead/http-nu";
    changelog = "https://github.com/cablehead/http-nu/blob/v${finalAttrs.version}/changes/v${finalAttrs.version}.md";
    license = lib.licenses.mit;
    maintainers = with lib.maintainers; [ cablehead cboecking ];
    mainProgram = "http-nu";
  };
})
```

## Initial Package Submission

### 1. Setup

```bash
git clone https://github.com/YOUR_USERNAME/nixpkgs.git
cd nixpkgs
git remote add upstream https://github.com/NixOS/nixpkgs.git
git switch --create http-nu upstream/master
mkdir -p pkgs/by-name/ht/http-nu
```

### 2. Get Hashes

Use placeholder hashes, then build to get correct ones:

```bash
# Set hash = lib.fakeHash; and cargoHash = lib.fakeHash;
nix build .#http-nu  # Fails with correct source hash
# Update source hash
nix build .#http-nu  # Fails with correct cargo hash
# Update cargo hash
nix build .#http-nu  # Should succeed
./result/bin/http-nu --version
```

### 3. Submit PR

Commit and push:

```bash
git add pkgs/by-name/ht/http-nu/package.nix
git commit -m "http-nu: init at 0.5.0"
git push --set-upstream origin http-nu
```

Create PR with this template:

```markdown
Diff: https://github.com/cablehead/http-nu/compare/v0.0.0...v0.5.0
Changelog: https://github.com/cablehead/http-nu/blob/v0.5.0/changes/v0.5.0.md

## Things done

- Built on platform:
  - [x] x86_64-linux
  - [ ] aarch64-linux
  - [ ] x86_64-darwin
  - [ ] aarch64-darwin
- Tested, as applicable:
  - [x] [Package tests] at `passthru.tests`.
- [x] Tested basic functionality of all binary files, usually in `./result/bin/`.
- [x] Fits [CONTRIBUTING.md], [pkgs/README.md], [maintainers/README.md] and other READMEs.
```

## Version Updates

### 1. Sync and Update

```bash
cd nixpkgs
git fetch upstream
git switch --create update-http-nu upstream/master
```

Update version in `package.nix`, then get new hashes:

```bash
# Set both hashes to placeholder value like sha256-AAAA...
nix build .#http-nu  # Get source hash from error
# Update source hash
nix build .#http-nu  # Get cargo hash from error
# Update cargo hash
nix build .#http-nu  # Verify build
./result/bin/http-nu --version
```

### 2. Submit Update PR

```bash
git add pkgs/by-name/ht/http-nu/package.nix
git commit -m "http-nu: 0.5.0 -> 0.9.0"
git push --set-upstream origin update-http-nu
```

PR template for updates:

```markdown
Diff: https://github.com/cablehead/http-nu/compare/v0.5.0...v0.9.0
Changelog: https://github.com/cablehead/http-nu/blob/v0.9.0/changes/v0.9.0.md

## Things done

- Built on platform:
  - [x] x86_64-linux
  - [ ] aarch64-linux
  - [ ] x86_64-darwin
  - [ ] aarch64-darwin
- Tested, as applicable:
  - [x] [Package tests] at `passthru.tests`.
- [x] Tested basic functionality of all binary files, usually in `./result/bin/`.
- [x] Fits [CONTRIBUTING.md], [pkgs/README.md], [maintainers/README.md] and other READMEs.
```

Maintainers can merge with: `@NixOS/nixpkgs-merge-bot merge`

## Adding Maintainers

Get your GitHub ID:

```bash
curl -s https://api.github.com/users/YOUR_USERNAME | jq .id
```

Add to `maintainers/maintainer-list.nix` (alphabetically):

```nix
yourusername = {
  email = "your@email.com";
  github = "yourusername";
  githubId = 12345678;
  name = "Your Name";
};
```

**Important**: Each maintainer requires a separate commit:

```bash
git commit -m "maintainers: add user1"
git commit -m "maintainers: add user2"
```

## Key Lessons from http-nu PR

Lessons from [#458947](https://github.com/NixOS/nixpkgs/pull/458947). These patterns are required by nixpkgs reviewers:

1. **Use `finalAttrs` pattern** instead of `rec` to avoid infinite recursion
2. **Use `tag` not `rev`** when fetching from git tags
3. **Use `rustPlatform.bindgenHook`** for Darwin builds needing libclang
4. **Omit `platforms`** - `buildRustPackage` handles this automatically
5. **Disable tests with documentation** when they need network/filesystem access
6. **Run treefmt** before committing: `nix fmt`
7. **Squash review fixes** into original commit, don't add "fix review" commits

## Resources

- [nixpkgs Rust guide](https://ryantm.github.io/nixpkgs/languages-frameworks/rust/)
- [Contributing guide](https://github.com/NixOS/nixpkgs/blob/master/CONTRIBUTING.md)
- [Package search](https://search.nixos.org/packages)
- [PR tracker](https://nixpk.gs/pr-tracker.html)
