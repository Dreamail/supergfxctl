{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    devenv.url = "github:cachix/devenv";
  };

  outputs =
    {
      nixpkgs,
      flake-parts,
      ...
    }@inputs:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        inputs.devenv.flakeModule
      ];

      systems = nixpkgs.lib.systems.flakeExposed;
      perSystem =
        {
          pkgs,
          ...
        }:
        {
          _module.args = {
            pkgs = import nixpkgs {
              config.allowUnfree = true;
            };
          };
          devenv.shells.default = {
            packages = with pkgs; [
              pkg-config
              systemd
            ];
            languages.rust = {
              enable = true;
            };
          };
        };
    };
}
