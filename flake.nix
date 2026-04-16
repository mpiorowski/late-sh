{
  description = "late.sh — a social SSH terminal";

  inputs = {
    # For listing and iterating nix systems
    flake-utils.url = "github:numtide/flake-utils";

    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    # For installing non-standard rustc versions
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    rust-overlay,
  }:
    {
      overlays.default = final: prev: {
        late-sh = self.packages.${final.stdenv.hostPlatform.system}.late-sh;
      };
    }
    // (flake-utils.lib.eachSystem nixpkgs.lib.systems.flakeExposed (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [
          rust-overlay.overlays.default
        ];
      };

      # When we're running in the shell, we want rustc with a bunch of extra
      # bits so that rust-analyzer, clippy, rustfmt, etc. all work out of the
      # box.
      rustShellToolchain = pkgs.rust-bin.stable.latest.default.override {
        # NOTE: explicitly add rust-src to the rustc compiler only in devShell.
        # this in turn causes a dependency on the rust compiler src, which
        # bloats the closure size by several GiB. but doing this here and not
        # by default avoids the default flake install from including that
        # dependency, so it's worth it.
        extensions = [
          "rust-src"
          "rust-analyzer"
          "llvm-tools-preview"
        ];
      };

      # But, whenever we are running CI builds or checks, we want to use a
      # smaller closure. This reduces the CI impact on fresh clones/VMs, etc.
      rustMinimalPlatform = let
        platform = pkgs.rust-bin.stable.latest.minimal;
      in
        pkgs.makeRustPlatform {
          rustc = platform;
          cargo = platform;
        };
    in {
      formatter = pkgs.alejandra;

      packages = {
        late-sh = pkgs.callPackage ./default.nix {
          rustPlatform = rustMinimalPlatform;
          gitRev = self.rev or self.dirtyRev or null;
        };
        default = self.packages.${system}.late-sh;
      };

      checks.late-sh = self.packages.${system}.late-sh.overrideAttrs ({...}: {
        # The default Rust infrastructure runs all builds in the release
        # profile, which is significantly slower. Run this under the `test`
        # profile instead
        cargoBuildType = "test";
        cargoCheckType = "test";
        buildPhase = "true";
        installPhase = "touch $out";
      });

      devShells.default = let
        packages = with pkgs; [
          rustShellToolchain
          llvmPackages.llvm # for e.g. llvm-symbolizer

          # Matches .mise.toml / CONTRIBUTING tooling
          mold
          cargo-nextest
          typos

          # Commonly useful cargo helpers
          cargo-llvm-cov
          cargo-watch
          cargo-autoinherit
          cargo-msrv

          # late-web frontend tooling
          nodejs
          tailwindcss_4

          # Integration / infra tooling (docker-compose stack, migrations, etc.)
          postgresql
          docker-compose

          # Commit signing
          gnupg
        ];

        # on macOS and Linux, use faster parallel linkers that are much more
        # efficient than the defaults. these noticeably improve link time even
        # for medium sized rust projects.
        rustLinkerFlags =
          if pkgs.stdenv.isLinux
          then ["-fuse-ld=mold" "-Wl,--compress-debug-sections=zstd"]
          else if pkgs.stdenv.isDarwin
          then
            # on darwin, /usr/bin/ld actually looks at the environment variable
            # $DEVELOPER_DIR, which is set by the nix stdenv, and if set,
            # automatically uses it to route the `ld` invocation to the binary
            # within. in the devShell though, that isn't what we want; it's
            # functional, but Xcode's linker as of ~v15 (not yet open source)
            # is ultra-fast and very shiny; it is enabled via -ld_new, and on by
            # default as of v16+
            ["--ld-path=$(unset DEVELOPER_DIR; /usr/bin/xcrun --find ld)" "-ld_new"]
          else [];

        rustLinkFlagsString =
          pkgs.lib.concatStringsSep " "
          (pkgs.lib.concatMap (x: ["-C" "link-arg=${x}"]) rustLinkerFlags);

        # The `RUSTFLAGS` environment variable is set in `shellHook` instead of
        # `env` to allow the `xcrun` command above to be interpreted by the
        # shell.
        shellHook = ''
          export RUSTFLAGS="${rustLinkFlagsString}"
        '';

        late-sh = self.packages.${system}.late-sh;
      in
        pkgs.mkShell {
          name = "late-sh";
          packages = packages ++ late-sh.nativeBuildInputs ++ late-sh.buildInputs;
          inherit shellHook;
        };
    }));
}
