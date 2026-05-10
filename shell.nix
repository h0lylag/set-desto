{
  pkgs ? import <nixpkgs> { },
}:
let
  mainPackage = pkgs.callPackage ./default.nix { };
in
pkgs.mkShell {
  inputsFrom = [ mainPackage ];

  packages = with pkgs; [
    rust-analyzer
    clippy
    rustfmt
  ];

  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath mainPackage.passthru.runtimeLibs;
}
