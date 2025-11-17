{
  inputs = {
    systems.url = "github:nix-systems/default";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    devshell.url = "github:numtide/devshell";

    naersk.url = "github:nmattia/naersk";
  };

  outputs = inputs@{ systems, flake-parts, ... }: (
    flake-parts.lib.mkFlake { inherit inputs; } ({
      systems = import systems;
      imports = [ inputs.devshell.flakeModule ];

      perSystem = { pkgs, ... }: {
        packages.default = let
          naersk-lib = pkgs.callPackage inputs.naersk {};
        in naersk-lib.buildPackage ./.;

        devshells.default =  {
          packages = with pkgs; [
            rustc # important for good editor support somehow
            gcc # necessary for `cargo build/run`

            cargo
            rustPackages.clippy
          ];
        };
      };
    })
  );
}
