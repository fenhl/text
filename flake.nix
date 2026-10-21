{
    inputs = {
        flake.url = "github:fenhl/flake";
        rust-overlay = {
            url = "github:oxalica/rust-overlay";
            inputs.nixpkgs.follows = "flake/nixpkgs";
        };
    };
    outputs = attrs: attrs.flake.lib {
        overlays = [
            attrs.rust-overlay.overlays.default
        ];
        devShells.default = { pkgs, ... }: {
            packages = with pkgs; [
                rust-bin.nightly.latest.default # nightly cargo, required for unstable feature const_trait_impl
            ];
        };
    };
}
