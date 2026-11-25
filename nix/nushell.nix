# The `nu` the dev shell runs the tests/*.nu suites with. It must be the
# engine http-nu embeds: the nu-protocol version in Cargo.lock, the same rule
# CLAUDE.md gives for hustcer/setup-nu in .github/workflows/ci.yml.
#
# nixpkgs' nushell is used whenever it already has that version. Otherwise
# `src` (the flake's `nushell` input, fixed to a release tag) is built, with
# its crates fetched from its own Cargo.lock, so the tag in flake.nix is the
# only thing to change on a bump. Evaluation fails while the two disagree.
{
  lib,
  nushell,
  rustPlatform,
  src,
}:

let
  lock = lib.importTOML ../Cargo.lock;
  embedded = (lib.findFirst (p: p.name == "nu-protocol") null lock.package).version;
  version = (lib.importTOML "${src}/Cargo.toml").workspace.package.version;
in
if nushell.version == embedded then
  nushell
else if version == embedded then
  nushell.overrideAttrs {
    inherit version src;
    # nixpkgs' patches and test skip list target its own version.
    patches = [ ];
    cargoPatches = [ ];
    cargoDeps = rustPlatform.importCargoLock { lockFile = "${src}/Cargo.lock"; };
    doCheck = false;
  }
else
  throw "http-nu embeds nushell ${embedded}, but the flake's `nushell` input is ${version}: set its tag in flake.nix to ${embedded} and run `nix flake lock`"
