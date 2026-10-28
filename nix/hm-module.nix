# Home-manager module for Hushflow speech-to-text
#
# Provides a systemd user service for autostart.
# Usage: imports = [ hushflow.homeManagerModules.default ];
#        services.handy.enable = true;
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.handy;
in
{
  options.services.handy = {
    enable = lib.mkEnableOption "Hushflow speech-to-text user service";

    package = lib.mkOption {
      type = lib.types.package;
      defaultText = lib.literalExpression "hushflow.packages.\${system}.handy";
      description = "The Hushflow package to use.";
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.user.services.handy = {
      Unit = {
        Description = "Hushflow speech-to-text";
        After = [ "graphical-session.target" ];
        PartOf = [ "graphical-session.target" ];
      };
      Service = {
        ExecStart = "${cfg.package}/bin/handy";
        Restart = "on-failure";
        RestartSec = 5;
      };
      Install.WantedBy = [ "graphical-session.target" ];
    };
  };
}
