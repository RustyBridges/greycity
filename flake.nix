{
  description = "Grey City - an ASCII roguelike life simulator";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f system);
      perSystem = system:
      let
        pkgs = import nixpkgs { inherit system; };
        lib = pkgs.lib;

        # macroquad/miniquad load these with dlopen at runtime, so they have to
        # be on LD_LIBRARY_PATH rather than just linked.
        runtimeLibs = lib.optionals pkgs.stdenv.isLinux (with pkgs; [
          libGL
          libx11
          libxi
          libxcursor
          libxrandr
          libxkbcommon
        ]);

        greycity = pkgs.rustPlatform.buildRustPackage {
          pname = "greycity";
          version = "0.1.0";
          src = lib.cleanSource ./.;
          cargoLock.lockFile = ./Cargo.lock;

          # Build the game binary and the headless simulator.
          cargoBuildFlags = [ "--workspace" ];
          cargoTestFlags = [ "-p" "greycity-sim" ];

          nativeBuildInputs = [ pkgs.makeWrapper ];
          buildInputs = runtimeLibs;

          postFixup = lib.optionalString pkgs.stdenv.isLinux ''
            wrapProgram $out/bin/greycity \
              --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtimeLibs}
          '';

          meta = {
            description = "ASCII roguelike life simulator set in a grey, living city";
            mainProgram = "greycity";
            platforms = lib.platforms.unix;
          };
        };
      in
      {
        package = greycity;

        apps.default = {
          type = "app";
          program = "${greycity}/bin/greycity";
        };
        apps.headless = {
          type = "app";
          program = "${greycity}/bin/headless";
        };

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [ cargo rustc rust-analyzer clippy rustfmt ] ++ runtimeLibs;
          LD_LIBRARY_PATH = lib.makeLibraryPath runtimeLibs;
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
        };
      };
    in
    {
      packages = forAllSystems (s: { default = (perSystem s).package; });
      apps = forAllSystems (s: (perSystem s).apps);
      devShells = forAllSystems (s: (perSystem s).devShells);
    };
}
