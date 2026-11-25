# The Datastar and Rocket bundle versions, read from the source at evaluation
# time. src/logging.rs's DATASTAR_VERSION and DATASTAR_ROCKET_VERSION are the
# reference; `check` fails if any other literal copy disagrees with them (see
# docs/how-to/refresh-datastar-sdk.md) or if a vendored bundle is missing.
{ lib }:

let
  src = ../src;
  lines = file: lib.splitString "\n" (builtins.readFile file);

  constant =
    name:
    let
      found = lib.concatMap (
        l:
        let
          m = builtins.match ''.*pub const ${name}: &str = "([^"]+)";.*'' l;
        in
        lib.optional (m != null) (builtins.head m)
      ) (lines "${src}/logging.rs");
    in
    if found == [ ] then
      throw "nix/datastar.nix: ${name} not found in src/logging.rs"
    else
      builtins.head found;

  datastar = constant "DATASTAR_VERSION";
  rocket = constant "DATASTAR_ROCKET_VERSION";

  # Every `datastar@V` / `datastar-rocket@V` literal; a line naming the Rocket
  # bundle belongs to Rocket's version, any other to core Datastar's.
  copies =
    path:
    lib.concatMap (
      l:
      let
        m = builtins.match ''.*datastar(-rocket)?@([^/"]+)(\.js|/).*'' l;
        isRocket = lib.hasInfix "datastar-rocket" l;
      in
      lib.optional (m != null) {
        inherit path;
        line = lib.trim l;
        found = builtins.elemAt m 1;
        want = if isRocket then rocket else datastar;
      }
    ) (lines "${src}/${path}");

  mismatches = lib.filter (c: c.found != c.want) (
    copies "handler.rs" ++ copies "stdlib/datastar/mod.nu"
  );

  bundles =
    lib.concatMap
      (n: [
        "stdlib/datastar/${n}.js"
        "stdlib/datastar/${n}.js.br"
      ])
      [
        "datastar@${datastar}"
        "datastar-rocket@${rocket}"
      ];
  missing = lib.filter (p: !builtins.pathExists "${src}/${p}") bundles;

  problems =
    map (c: "src/${c.path}: ${c.found} (want ${c.want}): ${c.line}") mismatches
    ++ map (p: "src/${p}: missing") missing;
in
{
  inherit datastar rocket;

  # Forces the comparison; a string, so it can be interpolated into a check.
  check =
    if problems == [ ] then
      "datastar ${datastar}, datastar-rocket ${rocket}"
    else
      throw ''
        Datastar versions disagree with src/logging.rs (datastar ${datastar}, datastar-rocket ${rocket}):
        ${lib.concatStringsSep "\n" problems}'';
}
