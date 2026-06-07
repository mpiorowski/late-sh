{
  lib,
  stdenv,
  rustPlatform,
  gitRev ? null,
  packageName ? "late-sh",
  packageDescription ? "Social SSH terminal — late.sh",
  mainProgram ? "late-ssh",
  cargoBuildFlags ? ["--workspace" "--bins"],
  fetchurl,
  pkg-config,
  cmake,
  perl,
  unzip,
  makeWrapper ? null,
  alsa-lib,
  glib-networking ? null,
  gst_all_1 ? null,
  gtk3 ? null,
  mold,
  webkitgtk_4_1 ? null,
}: let
  packageVersion = (builtins.fromTOML (builtins.readFile ./late-ssh/Cargo.toml)).package.version;
  gstPluginsBadNoLv2 =
    if stdenv.isLinux
    then
      gst_all_1.gst-plugins-bad.overrideAttrs (oldAttrs: {
        mesonFlags = (oldAttrs.mesonFlags or []) ++ ["-Dlv2=disabled"];
      })
    else null;
  gstreamerPlugins =
    if stdenv.isLinux
    then
      with gst_all_1; [
        gstreamer.out
        gst-plugins-base
        gst-plugins-good
        gstPluginsBadNoLv2
        gst-plugins-ugly
        gst-libav
      ]
    else [];
  gstreamerPluginPath = lib.makeSearchPath "lib/gstreamer-1.0" gstreamerPlugins;
  gstreamerPluginScanner =
    if stdenv.isLinux
    then "${gst_all_1.gstreamer.out}/libexec/gstreamer-1.0/gst-plugin-scanner"
    else "";
  webkitGstreamerSandboxPath =
    if stdenv.isLinux
    then lib.concatStringsSep ":" (map toString gstreamerPlugins)
    else "";
  filterSrc = src: regexes:
    lib.cleanSourceWith {
      inherit src;
      filter = path: type: let
        relPath = lib.removePrefix (toString src + "/") (toString path);
      in
        lib.all (re: builtins.match re relPath == null) regexes;
    };
  livekitWebrtc = let
    archives = {
      x86_64-linux = {
        triple = "linux-x64-release";
        hash = "sha256-3OnZQUzY4syaRxwFRIGFGyV60qh5ESzlvlY2m0/v2u0=";
      };
      aarch64-linux = {
        triple = "linux-arm64-release";
        hash = "sha256-tVymLCixjcW7cgpwgq5GjXjyE8o5vMz4QZXy/ljP5xM=";
      };
    };
  in
    if builtins.hasAttr stdenv.hostPlatform.system archives
    then builtins.getAttr stdenv.hostPlatform.system archives
    else throw "unsupported LiveKit WebRTC platform for Nix: ${stdenv.hostPlatform.system}";
  livekitWebrtcZip =
    if stdenv.isLinux
    then
      fetchurl {
        url = "https://github.com/livekit/rust-sdks/releases/download/webrtc-51ef663/webrtc-${livekitWebrtc.triple}.zip";
        hash = livekitWebrtc.hash;
      }
    else null;
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
        unzip
      ];

    buildInputs =
      lib.optionals stdenv.isLinux [
        alsa-lib
        glib-networking
        gtk3
        webkitgtk_4_1
      ]
      ++ lib.optionals stdenv.isLinux gstreamerPlugins;

    # webrtc-sys downloads this archive in build.rs by default. Nix builds are
    # sandboxed, so provide it up front and point the build script at it.
    preBuild = lib.optionalString stdenv.isLinux ''
      mkdir -p "$TMPDIR/livekit-webrtc"
      unzip -q "${livekitWebrtcZip}" -d "$TMPDIR/livekit-webrtc"
      export LK_CUSTOM_WEBRTC="$TMPDIR/livekit-webrtc/${livekitWebrtc.triple}"
      test -f "$LK_CUSTOM_WEBRTC/lib/libwebrtc.a"
      test -f "$LK_CUSTOM_WEBRTC/webrtc.ninja"
      test -f "$LK_CUSTOM_WEBRTC/desktop_capture.ninja"
    '';

    # The embedded CLI YouTube helper uses WebKitGTK + GStreamer. WebKit
    # discovers codecs, sinks, and TLS modules at runtime from WebKit helper
    # processes, so a Nix-built binary must carry those search paths and the
    # paths WebKit should expose inside its web-process sandbox.
    postFixup = lib.optionalString stdenv.isLinux ''
      if [ -x "$out/bin/late" ]; then
        wrapProgram "$out/bin/late" \
          --set GST_PLUGIN_SYSTEM_PATH_1_0 "${gstreamerPluginPath}" \
          --set GST_PLUGIN_SCANNER "${gstreamerPluginScanner}" \
          --set LATE_WEBKIT_GSTREAMER_SANDBOX_PATHS "${webkitGstreamerSandboxPath}" \
          --prefix GIO_EXTRA_MODULES : "${glib-networking}/lib/gio/modules"
      fi
      if [ -x "$out/bin/late-cli" ]; then
        wrapProgram "$out/bin/late-cli" \
          --set GST_PLUGIN_SYSTEM_PATH_1_0 "${gstreamerPluginPath}" \
          --set GST_PLUGIN_SCANNER "${gstreamerPluginScanner}" \
          --set LATE_WEBKIT_GSTREAMER_SANDBOX_PATHS "${webkitGstreamerSandboxPath}" \
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
