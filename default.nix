{
  pkgs ? import <nixpkgs> { },
}:

let
  manifest = (pkgs.lib.importTOML ./Cargo.toml).package;

  runtimeLibs = with pkgs; [
    stdenv.cc.cc.lib
    libGL
    libxkbcommon
    wayland
    libx11
    libxcursor
    libxrandr
    libxi
  ];
in

pkgs.rustPlatform.buildRustPackage rec {
  pname = manifest.name;
  version = manifest.version;

  cargoLock.lockFile = ./Cargo.lock;

  src = pkgs.lib.cleanSource ./.;

  nativeBuildInputs = with pkgs; [
    pkg-config
    autoPatchelfHook
  ];

  buildInputs = runtimeLibs;

  runtimeDependencies = runtimeLibs;

  postInstall = ''
    install -Dm644 assets/com.h0lylag.setdesto.desktop $out/share/applications/set-desto.desktop
    install -Dm644 assets/com.h0lylag.setdesto.svg $out/share/icons/hicolor/scalable/apps/com.h0lylag.setdesto.svg
    install -Dm644 assets/com.h0lylag.setdesto.metainfo.xml $out/share/metainfo/com.h0lylag.setdesto.metainfo.xml
  '';

  passthru = {
    inherit runtimeLibs;
  };

  meta = with pkgs.lib; {
    description = "Set Desto desktop utility";
    license = licenses.mit;
    platforms = platforms.linux;
    mainProgram = "set-desto";
  };
}
