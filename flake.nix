{
  description = "opnsense unbound external-dns webhook";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs";
    crane = {
        url = "github:ipetkov/crane";
        inputs.nixpkgs.follows = "nixpkgs";
    };

    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-utils.url = "github:numtide/flake-utils";
    
    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, crane, fenix, flake-utils, advisory-db }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };

        inherit (pkgs) lib;

        craneLib = crane.lib.${system};
        src = craneLib.cleanCargoSource (craneLib.path ./.);

        commonArgs = {
          inherit src;
          strictDeps = true;
    
          nativeBuildInputs = [
            pkgs.pkg-config
          ];

          buildInputs = [
            pkgs.openssl
          ];
        };

        craneLibLlvmTools = craneLib.overrideToolchain
          (fenix.packages.${system}.complete.withComponents [
            "cargo"
            "llvm-tools"
            "rustc"
          ]);

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        opnsense_unbound_external-dns_webhook = craneLib.buildPackage (commonArgs // {
            inherit cargoArtifacts;
        });

        opnsense_unbound_external-dns_webhook-image = pkgs.dockerTools.buildImage {
          name = "opnsense_unbound_external-dns_webhook";
          config = {
            Cmd = [ "${opnsense_unbound_external-dns_webhook}/bin/opnsense_unbound_external-dns_webhook" ];
          };
        };
      in {
        packages = {
            default = opnsense_unbound_external-dns_webhook;
            image = opnsense_unbound_external-dns_webhook-image;
        };

        checks = {
          inherit opnsense_unbound_external-dns_webhook;

          opnsense_unbound_external-dns_webhook-clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          });

          opnsense_unbound_external-dns_webhook-fmt = craneLib.cargoFmt (commonArgs // {
            inherit src;
          });

          opnsense_unbound_external-dns_webhook-audit = craneLib.cargoAudit (commonArgs // {
            inherit src advisory-db;
          });       
        };

        apps.default = flake-utils.lib.mkApp {
            drv = opnsense_unbound_external-dns_webhook;
        };

        devShells.default = craneLib.devShell {
          packages = [
            pkgs.rust-analyzer
          ];
        };
      }
    );
}
