{
  description = "Logia - A modern AI-powered workspace";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      # Apply overlay to fix ctranslate2 build (missing cstdint header)
      pkgs = import nixpkgs {
        inherit system;
        overlays = [
          (final: prev: {
            ctranslate2 = prev.ctranslate2.overrideAttrs (oldAttrs: {
              env =
                (oldAttrs.env or {})
                // {
                  CXXFLAGS = (oldAttrs.env.CXXFLAGS or "") + " -include cstdint";
                };
            });
          })
        ];
      };

      # Python environment with faster-whisper for transcription
      pythonEnv = pkgs.python312.withPackages (ps:
        with ps; [
          faster-whisper
        ]);

      # Runtime libraries needed by WebKitGTK and Tauri
      runtimeLibs = with pkgs; [
        at-spi2-atk
        atkmm
        cairo
        gdk-pixbuf
        glib
        gtk3
        harfbuzz
        librsvg
        libsoup_3
        pango
        webkitgtk_4_1
        openssl
        glib-networking
        gsettings-desktop-schemas
      ];

      # Build-time dependencies
      nativeBuildDeps = with pkgs; [
        pkg-config
        gobject-introspection
        wrapGAppsHook4
        ffmpeg
        pulseaudio
        cargo-tauri
        gcc
      ];
    in {
      packages = {
        logia = pkgs.rustPlatform.buildRustPackage rec {
            pname = "logia";
            version = "0.10.0";

            src = ./.;

            # Point to the Cargo workspace in src-tauri
            cargoRoot = "src-tauri";
            buildAndTestSubdir = "src-tauri";

            cargoLock = {
              lockFile = ./src-tauri/Cargo.lock;
            };

            nativeBuildInputs = nativeBuildDeps ++ [pkgs.pkg-config];
            buildInputs = runtimeLibs ++ [pythonEnv pkgs.openssl];

            # Environment variables for the build
            OPENSSL_NO_VENDOR = 1;

            # Override the default build phase to use cargo-tauri properly
            buildPhase = ''
              runHook preBuild

              echo "Frontend dist contents:"
              ls -la dist/

              cd src-tauri

              # Create a config override file to skip beforeBuildCommand
              cat > tauri.nix.conf.toml << 'EOF'
              [build]
              beforeBuildCommand = ""
              beforeDevCommand = ""
              EOF

              # Use cargo-tauri build with config override file
              cargo tauri build --no-bundle --ci --config tauri.nix.conf.toml

              cd ..

              runHook postBuild
            '';

            installPhase = ''
              runHook preInstall

              mkdir -p $out/bin
              cp src-tauri/target/release/logia $out/bin/

              # Install transcription resources
              mkdir -p $out/share/logia/transcription
              cp src-tauri/src/audio/transcription/transcribe.py $out/share/logia/transcription/
              cp src-tauri/src/audio/transcription/requirements.txt $out/share/logia/transcription/

              # Install desktop file
              mkdir -p $out/share/applications
              cat > $out/share/applications/logia.desktop << EOF
              [Desktop Entry]
              Name=Logia
              Comment=A modern AI-powered workspace
              Exec=$out/bin/logia
              Icon=logia
              Terminal=false
              Type=Application
              Categories=Office;Productivity;
              EOF

              # Install icons if they exist
              if [ -d "src-tauri/icons" ]; then
                mkdir -p $out/share/icons/hicolor/128x128/apps
                cp src-tauri/icons/128x128.png $out/share/icons/hicolor/128x128/apps/logia.png 2>/dev/null || true
              fi

              runHook postInstall
            '';

            postFixup = ''
              wrapProgram $out/bin/logia \
                --set LOGIA_PYTHON_PATH "${pythonEnv}/bin/python" \
                --set LOGIA_TRANSCRIBE_SCRIPT "$out/share/logia/transcription/transcribe.py" \
                --prefix PATH : "${pkgs.lib.makeBinPath [pkgs.ffmpeg pkgs.pulseaudio pkgs.uv]}" \
                --prefix XDG_DATA_DIRS : "${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}" \
                --prefix XDG_DATA_DIRS : "${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}" \
                --set GIO_MODULE_DIR "${pkgs.glib-networking}/lib/gio/modules/"
            '';

            meta = with pkgs.lib; {
              description = "A modern AI-powered workspace";
              homepage = "https://github.com/daschinmoy21/Logia";
              license = licenses.mit;
              maintainers = [];
              platforms = platforms.linux;
              mainProgram = "logia";
            };
          };

        default = self.packages.${system}.logia;
      };

      # Development shell (still uses bun for live development)
      devShells.default = pkgs.mkShell {
        nativeBuildInputs =
          nativeBuildDeps
          ++ [
            pythonEnv
            pkgs.cargo
            pkgs.rustc
            pkgs.bun
            pkgs.nodejs
            pkgs.uv
            pkgs.xdg-utils
            pkgs.pkg-config
          ];

        buildInputs = runtimeLibs ++ [pkgs.openssl];

        OPENSSL_NO_VENDOR = 1;
        PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";

        shellHook = ''
          export XDG_DATA_DIRS=${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}:$XDG_DATA_DIRS
          export LOGIA_PYTHON_PATH="$(which python)"
          export GIO_MODULE_DIR="${pkgs.glib-networking}/lib/gio/modules/"
          export XDG_RUNTIME_DIR=''${XDG_RUNTIME_DIR:-/run/user/$(id -u)}
          export DBUS_SESSION_BUS_ADDRESS=''${DBUS_SESSION_BUS_ADDRESS:-unix:path=$XDG_RUNTIME_DIR/bus}

          echo "🚀 Logia development shell loaded!"
          echo "   Run 'bun install' then 'bun tauri dev' to start development"
          echo "   Run 'bun run build' to rebuild the frontend (commit dist/ for Nix builds)"
        '';
      };

      apps.default = flake-utils.lib.mkApp {
        drv = self.packages.${system}.default;
      };
    });
}
