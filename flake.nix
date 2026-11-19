{
  description = "A Nushell-scriptable HTTP server";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    # The dev shell's `nu`, used when nixpkgs lacks the version http-nu embeds.
    # Keep its tag equal to nu-protocol in Cargo.lock (see nix/nushell.nix).
    nushell = {
      url = "github:nushell/nushell/0.117.0";
      flake = false;
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      nushell,
    }:
    let
      inherit (nixpkgs) lib;
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems = f: lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      overlays.default = final: _prev: {
        http-nu = final.callPackage ./nix/package.nix { };
      };

      packages = forAllSystems (
        pkgs:
        let
          http-nu = pkgs.callPackage ./nix/package.nix { };
        in
        {
          default = http-nu;
          inherit http-nu;
          nushell = pkgs.callPackage ./nix/nushell.nix { src = nushell; };
        }
      );

      devShells = forAllSystems (
        pkgs:
        let
          inherit (self.packages.${pkgs.stdenv.hostPlatform.system}) http-nu nushell;
        in
        {
          default = pkgs.mkShell {
            inputsFrom = [ http-nu ];
            packages = [
              nushell
              pkgs.cargo
              pkgs.rustc
              pkgs.clippy
              pkgs.rustfmt
              pkgs.rust-analyzer
              pkgs.deno
              pkgs.brotli
            ];

            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";

            # Interactive `nix develop` drops into Nushell (the engine version
            # the binary embeds). `nix develop --command`, direnv, and an
            # already-running nu keep the calling shell; set
            # HTTP_NU_DEVSHELL_BASH=1 to stay in bash.
            shellHook = ''
              if [[ $- == *i* && -z "''${HTTP_NU_DEVSHELL_BASH:-}" && -z "''${NU_VERSION:-}" ]]; then
                exec ${lib.getExe nushell}
              fi
            '';
          };
        }
      );

      checks = forAllSystems (
        pkgs:
        let
          system = pkgs.stdenv.hostPlatform.system;
        in
        {
          inherit (self.packages.${system}) http-nu;
          devShell = self.devShells.${system}.default;
          # Fails evaluation if a Datastar version literal or bundle file
          # disagrees with src/logging.rs.
          datastar-versions =
            pkgs.writeText "datastar-versions"
              (import ./nix/datastar.nix { inherit lib; }).check;
        }
      );

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
