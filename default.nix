{
  lib,
  stdenv,
  rustPlatform,
  gitRev ? null,
  packageName ? "late-sh",
  packageDescription ? "Social SSH terminal — late.sh",
  mainProgram ? "late-ssh",
  cargoBuildFlags ? ["--workspace" "--bins"],
  pkg-config,
  cmake,
  perl,
  makeWrapper ? null,
  alsa-lib,
  glib-networking ? null,
  gst_all_1 ? null,
  gtk3 ? null,
  mold,
  webkitgtk_4_1 ? null,
}: let
  packageVersion = (builtins.fromTOML (builtins.readFile ./late-ssh/Cargo.toml)).package.version;
  gstreamerPlugins =
    if stdenv.isLinux
    then
      with gst_all_1; [
        gstreamer
        gst-plugins-base
        gst-plugins-good
        gst-plugins-bad
        gst-plugins-ugly
        gst-libav
      ]
    else [];
  gstreamerPluginPath = lib.makeSearchPath "lib/gstreamer-1.0" gstreamerPlugins;
  filterSrc = src: regexes:
    lib.cleanSourceWith {
      inherit src;
      filter = path: type: let
        relPath = lib.removePrefix (toString src + "/") (toString path);
      in
        lib.all (re: builtins.match re relPath == null) regexes;
    };
in
  rustPlatform.buildRustPackage {
    pname = packageName;
    version = "${packageVersion}-unstable-${
      if gitRev != null
      then gitRev
      else "dirty"
    }";

    # Build all deployable workspace binaries. late-web's CSS is a pre-built,
    # committed asset; tailwind is not invoked at build time.
    inherit cargoBuildFlags;
    useNextest = true;

    src = filterSrc ./. [
      ".*\\.nix$"
      "^.jj/"
      "^.git/"
      "^flake\\.lock$"
      "^target/"
      "^late-web/node_modules/"
    ];

    cargoLock.lockFile = ./Cargo.lock;

    nativeBuildInputs =
      [
        pkg-config
        cmake
        perl
        rustPlatform.bindgenHook
      ]
      ++ lib.optionals stdenv.isLinux [
        makeWrapper
        mold
      ];

    buildInputs =
      lib.optionals stdenv.isLinux [
        alsa-lib
        glib-networking
        gtk3
        webkitgtk_4_1
      ]
      ++ lib.optionals stdenv.isLinux gstreamerPlugins;

    # The embedded CLI YouTube helper uses WebKitGTK + GStreamer. WebKit
    # discovers codecs, sinks, and TLS modules at runtime, so a Nix-built
    # binary must carry those search paths with it.
    postFixup = lib.optionalString stdenv.isLinux ''
      if [ -x "$out/bin/late" ]; then
        wrapProgram "$out/bin/late" \
          --prefix GST_PLUGIN_SYSTEM_PATH_1_0 : "${gstreamerPluginPath}" \
          --prefix GIO_EXTRA_MODULES : "${glib-networking}/lib/gio/modules"
      fi
      if [ -x "$out/bin/late-cli" ]; then
        wrapProgram "$out/bin/late-cli" \
          --prefix GST_PLUGIN_SYSTEM_PATH_1_0 : "${gstreamerPluginPath}" \
          --prefix GIO_EXTRA_MODULES : "${glib-networking}/lib/gio/modules"
      fi
    '';

    # Integration tests require a live postgres; skip by default.
    doCheck = false;

    env = {
      RUST_BACKTRACE = 1;
      CARGO_INCREMENTAL = "0"; # https://github.com/rust-lang/rust/issues/139110
      RUSTFLAGS = lib.optionalString stdenv.isLinux "-C link-arg=-fuse-ld=mold";
      NIX_LATE_GIT_HASH = gitRev;
    };

    meta = {
      description = packageDescription;
      homepage = "https://github.com/mpiorowski/late-sh";
      # Source-available under FSL-1.1-MIT (converts to MIT after 2 years).
      license = {
        shortName = "FSL-1.1-MIT";
        fullName = "Functional Source License, Version 1.1, MIT Future License";
        url = "https://fsl.software/";
        free = true;
        redistributable = true;
      };
      inherit mainProgram;
    };
  }
