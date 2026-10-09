{
  description = "LanguaLinker Rust, Tauri, and Svelte development shell";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-darwin" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in {
      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
        in {
          default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              clippy
              nodejs_24
              pkg-config
              rustc
              rustfmt
              rustup
            ] ++ pkgs.lib.optionals stdenv.isLinux [
              atk
              cairo
              gdk-pixbuf
              glib
              gtk3
              libsoup_3
              pango
              webkitgtk_4_1
            ];

            shellHook = ''
              echo "LanguaLinker development shell"
              echo "Run: npm --prefix frontend install && npm --prefix frontend run check"
              echo "Run: cargo check --workspace"
            '';
          };
        });
    };
}