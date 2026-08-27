{
  description = "Prchum — review pull requests and patches";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      # Linux only. The macOS app is Swift and Xcode, built by its own
      # toolchain; there is nothing here for Nix to do on darwin that
      # would not be a worse version of `make app`.
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAll = function:
        nixpkgs.lib.genAttrs systems
          (system: function nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAll (pkgs: {
        default = self.packages.${pkgs.system}.prchum-gtk;

        prchum-gtk = pkgs.rustPlatform.buildRustPackage {
          pname = "prchum-gtk";
          version = "0.4.0";

          # The whole repository, because the shell path-depends on the
          # core crates a directory up.
          src = ./.;
          # The cargo project is a directory in, and its lockfile with
          # it; without cargoRoot the vendoring hook looks for one at the
          # top of the source and finds nothing.
          cargoRoot = "linux";
          buildAndTestSubdir = "linux";

          # Derived from the lockfile rather than a hash written here:
          # a hash would need bumping by hand on every dependency change,
          # and would be wrong quietly until someone noticed.
          cargoLock.lockFile = ./linux/Cargo.lock;

          nativeBuildInputs = with pkgs; [
            pkg-config
            # Sets up the runtime environment a GTK application needs —
            # icon themes, GSettings schemas — which it would otherwise
            # start without and complain about.
            wrapGAppsHook4
          ];

          buildInputs = with pkgs; [ glib gtk4 libadwaita ];

          postInstall = ''
            install -Dm644 linux/data/eu.dumontix.prchum.desktop \
              $out/share/applications/eu.dumontix.prchum.desktop
            install -Dm644 linux/data/eu.dumontix.prchum.metainfo.xml \
              $out/share/metainfo/eu.dumontix.prchum.metainfo.xml
            for size in 128 256 512; do
              install -Dm644 linux/data/icons/eu.dumontix.prchum-$size.png \
                $out/share/icons/hicolor/''${size}x''${size}/apps/eu.dumontix.prchum.png
            done
          '';

          meta = with pkgs.lib; {
            description = "Review pull requests and patches";
            homepage = "https://github.com/perrito666/prchum";
            license = licenses.mit;
            mainProgram = "prchum-gtk";
            platforms = platforms.linux;
          };
        };
      });

      # `nix develop` — the GTK stack and a Rust toolchain, so working on
      # the Linux shell does not mean having the right distribution or
      # building a virtual machine first.
      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.system}.prchum-gtk ];
          packages = with pkgs; [
            cargo
            rustc
            rust-analyzer
            clippy
            rustfmt
            # The tools prchum itself delegates to, so a shell can review
            # something as well as build it.
            git
            gh
          ];
        };
      });

      formatter = forAll (pkgs: pkgs.nixpkgs-fmt);
    };
}
