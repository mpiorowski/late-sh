{
  lib,
  stdenv,
  rustPlatform,
  gitRev ? null,
  pkg-config,
  cmake,
  perl,
  alsa-lib,
  mold,
}: let
  packageVersion = (builtins.fromTOML (builtins.readFile ./late-ssh/Cargo.toml)).package.version;
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
    pname = "late-sh";
    version = "${packageVersion}-unstable-${
      if gitRev != null
      then gitRev
      else "dirty"
    }";

    # Build all deployable workspace binaries. late-web's CSS is a pre-built,
    # committed asset; tailwind is not invoked at build time.
    cargoBuildFlags = ["--workspace" "--bins"];
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
        mold
      ];

    buildInputs = lib.optionals stdenv.isLinux [
      alsa-lib
    ];

    # Integration tests require a live postgres; skip by default.
    doCheck = false;

    env = {
      RUST_BACKTRACE = 1;
      CARGO_INCREMENTAL = "0"; # https://github.com/rust-lang/rust/issues/139110
      RUSTFLAGS = lib.optionalString stdenv.isLinux "-C link-arg=-fuse-ld=mold";
      NIX_LATE_GIT_HASH = gitRev;
    };

    meta = {
      description = "Social SSH terminal — late.sh";
      homepage = "https://github.com/mpiorowski/late-sh";
      # Source-available under FSL-1.1-MIT (converts to MIT after 2 years).
      license = {
        shortName = "FSL-1.1-MIT";
        fullName = "Functional Source License, Version 1.1, MIT Future License";
        url = "https://fsl.software/";
        free = true;
        redistributable = true;
      };
      mainProgram = "late-ssh";
    };
  }
