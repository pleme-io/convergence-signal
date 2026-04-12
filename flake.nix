{
  description = "Convergence signal — proof of convergence computing";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    substrate = {
      url = "github:pleme-io/substrate";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.fenix.follows = "fenix";
    };
    forge = {
      url = "github:pleme-io/forge";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.fenix.follows = "fenix";
      inputs.substrate.follows = "substrate";
      inputs.crate2nix.follows = "crate2nix";
    };
    crate2nix = {
      url = "github:nix-community/crate2nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, fenix, substrate, forge, crate2nix, ... }: let
    systems = ["aarch64-darwin" "x86_64-linux" "aarch64-linux"];
    eachSystem = f: nixpkgs.lib.genAttrs systems f;

    mkOutputs = system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ substrate.rustOverlays.${system}.rust ];
      };
      crate2nixBin = crate2nix.packages.${system}.default;
      forgePkg = forge.packages.${system}.default;

      rustService = import "${substrate}/lib/rust-service.nix" {
        inherit system nixpkgs;
        nixLib = substrate;
        crate2nix = crate2nixBin;
        forge = forgePkg;
      };

      outputs = rustService {
        serviceName = "convergence-signal";
        src = self;
        registry = "ghcr.io/pleme-io/convergence-signal";
        packageName = "convergence-signal";
        namespace = "convergence-signal";
        architectures = ["arm64"];
        nativeBuildInputs = [];
        ports = { http = 8080; };
      };
    in {
      packages = outputs.packages;
      devShells = outputs.devShells;
      apps = outputs.apps;
    };
  in {
    packages = eachSystem (system: (mkOutputs system).packages);
    devShells = eachSystem (system: (mkOutputs system).devShells);
    apps = eachSystem (system: (mkOutputs system).apps);
  };
}
