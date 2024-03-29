{ config, lib, pkgs, ... }:

let
  dockerRun =
    import ./default.nix { pkgs = pkgs; };

  cfg =
    config.services.dockerRun;

  commonEnvironment = {
    LC_ALL = "en_US.UTF-8";
    LOCALE_ARCHIVE = "${pkgs.glibcLocales}/lib/locale/locale-archive";
  };
in
{
  options = {
    services.dockerRun = {
      enable = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = "Whether to enable docker-run";
      };

      environment = lib.mkOption {
        type = lib.types.attrs;
        default = {};
        description = "Environment variables for the service";
      };
    };
  };

  config = lib.mkIf cfg.enable {
    # Add glot group
    users.groups.glot = {};

    # Service user
    users.extraUsers.glot = {
      isSystemUser = true;
      group = "glot";
      extraGroups = ["docker"];
      description = "service user";
    };

    # Systemd service
    systemd.services.docker-run = {
      description = "docker-run service";
      wantedBy = [ "multi-user.target" ];

      serviceConfig =
        {
          ExecStart = "${dockerRun}/bin/docker-run";
          Restart = "always";
          User = "glot";
        };

      environment = commonEnvironment // cfg.environment;
    };
  };
}
