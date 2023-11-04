{
  nixConfig.bash-prompt = "[nix-develop]$ ";

  inputs = {
    naersk.url = "github:nix-community/naersk";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
    htmx = {
      url = "https://unpkg.com/htmx.org@1.9.6/dist/htmx.min.js";
      flake = false;
    };
  };

  outputs = { nixpkgs, utils, naersk, htmx, ... }:
    utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        naersk-lib = pkgs.callPackage naersk { };
        nativeBuildInputs = [ pkgs.tailwindcss ];
        HTMX_OUT_PATH = htmx.outPath;
      in {
        defaultPackage = naersk-lib.buildPackage {
          inherit nativeBuildInputs HTMX_OUT_PATH;
          root = ./.;
        };
        devShell = with pkgs;
          mkShell {
            inherit nativeBuildInputs HTMX_OUT_PATH;
            buildInputs =
              [ cargo rustc rustfmt pre-commit rustPackages.clippy ];
            RUST_SRC_PATH = rustPlatform.rustLibSrc;
          };
      });
}
