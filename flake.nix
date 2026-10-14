{
  description = "rustle: dictation for Linux with a themed overlay";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};

      # Libraries the crates link against or dlopen at runtime.
      buildInputs = with pkgs; [
        pipewire
        wayland
        libxkbcommon
        vulkan-headers
        vulkan-loader
      ];
      nativeBuildInputs = with pkgs; [
        cmake
        pkg-config
        rustPlatform.bindgenHook
        shaderc
        git
      ];
      runtimeTools = with pkgs; [ wl-clipboard playerctl fontconfig pipewire ];
    in {
      packages.${system} = rec {
        rustle = pkgs.rustPlatform.buildRustPackage {
          pname = "rustle-dictation";
          version = "0.1.0";
          src = self;
          cargoLock.lockFile = ./Cargo.lock;
          inherit buildInputs;
          # cmake is only used by whisper-rs's build script, not as the package's own build system
          dontUseCmakeConfigure = true;
          doCheck = false;
          postFixup = ''
            wrapProgram $out/bin/rustle \
              --prefix PATH : ${pkgs.lib.makeBinPath runtimeTools} \
              --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath [ pkgs.vulkan-loader pkgs.wayland pkgs.libxkbcommon ]}
          '';
          nativeBuildInputs = nativeBuildInputs ++ [ pkgs.makeWrapper ];
          meta.mainProgram = "rustle";
        };
        default = rustle;
      };

      homeManagerModules = rec {
        rustle = import ./nix/home-manager.nix { inherit self; };
        default = rustle;
      };

      devShells.${system}.default = pkgs.mkShell {
        inherit buildInputs;
        nativeBuildInputs = nativeBuildInputs ++ (with pkgs; [ cargo rustc clippy rustfmt rust-analyzer ]);
        packages = runtimeTools;
        LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [ pkgs.vulkan-loader pkgs.wayland pkgs.libxkbcommon pkgs.pipewire ];
      };
    };
}
