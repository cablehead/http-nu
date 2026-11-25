{
  lib,
  stdenv,
  rustPlatform,
  pkg-config,
  aws-lc,
  libgit2,
  oniguruma,
  sqlite,
  zlib,
  zstd,
  versionCheckHook,
  # The `cross-stream` cargo feature (the --store / .cat / .append commands),
  # on by default as in Cargo.toml.
  withCrossStream ? true,
}:

let
  cargoToml = lib.importTOML ../Cargo.toml;
  datastarVersions = import ./datastar.nix { inherit lib; };
  fs = lib.fileset;
in
rustPlatform.buildRustPackage (finalAttrs: {
  pname = "http-nu";
  inherit (cargoToml.package) version;

  src = fs.toSource {
    root = ../.;
    fileset = fs.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../build.rs
      ../src
      ../syntaxes
      # Workspace member: cargo needs its manifest to resolve the workspace.
      ../tests/test_plugin
      # include_str!'d by src/store.rs
      ../examples/topic-placeholder.nu
    ];
  };

  cargoLock.lockFile = ../Cargo.lock;

  buildNoDefaultFeatures = !withCrossStream;

  cargoBuildFlags = [ "--package=http-nu" ];

  # Darwin needs bindgenHook for libproc
  nativeBuildInputs = [
    pkg-config
  ]
  ++ lib.optionals stdenv.hostPlatform.isDarwin [ rustPlatform.bindgenHook ];

  # nixpkgs sets AWS_LC_SYS_USE_SYSTEM, LIBGIT2_NO_VENDOR and
  # RUSTONIG_SYSTEM_LIBONIG, so these -sys crates link packaged libraries
  # instead of building bundled copies.
  buildInputs = [
    aws-lc
    libgit2
    oniguruma
    sqlite
    zlib
    zstd
  ];

  # The test suites start servers, spawn `nu`, and build a plugin; they run in
  # CI and the dev shell (scripts/check.sh), not in the sandbox.
  doCheck = false;

  doInstallCheck = true;
  nativeInstallCheckInputs = [ versionCheckHook ];
  versionCheckProgramArg = "--version";

  passthru = {
    inherit withCrossStream;
    # The embedded bundles' versions, as src/logging.rs declares them.
    datastarVersion = datastarVersions.datastar;
    datastarRocketVersion = datastarVersions.rocket;
  };

  meta = {
    description = cargoToml.package.description;
    homepage = cargoToml.package.homepage;
    license = lib.licenses.mit;
    mainProgram = "http-nu";
  };
})
