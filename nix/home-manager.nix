# Home Manager module: `services.oxivox.enable = true;`
{ self }:
{ config, lib, pkgs, ... }:
let
  cfg = config.services.oxivox;
  tomlFormat = pkgs.formats.toml { };
in
{
  options.services.oxivox = {
    enable = lib.mkEnableOption "the oxivox dictation daemon";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      defaultText = lib.literalExpression "oxivox.packages.\${pkgs.stdenv.hostPlatform.system}.default";
      description = "The oxivox package to use.";
    };

    settings = lib.mkOption {
      type = tomlFormat.type;
      default = { };
      example = lib.literalExpression ''
        {
          language = "en";
          media_ignore = [ "vlc" ];
          overlay.bottom_margin = 80;
        }
      '';
      description = ''
        Contents of {file}`$XDG_CONFIG_HOME/oxivox/config.toml`. Every key is optional; see
        `contrib/config.toml` in the oxivox repository for the keys and their defaults.
      '';
    };

    theme = lib.mkOption {
      type = lib.types.nullOr tomlFormat.type;
      default = null;
      example = {
        background = "#1a1b26";
        accent = "#7aa2f7";
      };
      description = ''
        Contents of {file}`$XDG_CONFIG_HOME/oxivox/theme.toml`. When null the file is not managed, so a theme
        generator (for example Noctalia) or you can write it; the overlay reloads it when it changes.
      '';
    };

    hyprland = {
      enable = lib.mkEnableOption "the oxivox key bindings in `wayland.windowManager.hyprland`";

      toggleBind = lib.mkOption {
        type = lib.types.str;
        default = "ALT, SPACE";
        example = "SUPER, D";
        description = "Hyprland `MODS, key` that starts and stops a dictation.";
      };
    };
  };

  config = lib.mkIf cfg.enable (lib.mkMerge [
    {
      home.packages = [ cfg.package ];

      systemd.user.services.oxivox = {
        Unit = {
          Description = "oxivox dictation daemon";
          PartOf = [ "graphical-session.target" ];
          After = [ "graphical-session.target" "pipewire.service" ];
        };
        Service = {
          ExecStart = "${lib.getExe cfg.package} daemon";
          Restart = "on-failure";
          RestartSec = 3;
          # oxivox shells out to wl-copy/wl-paste, playerctl, fc-match and pw-dump; wl-copy itself runs `cat` (coreutils).
          # This replaces the whole PATH, so everything needed must be listed
          Environment = [
            "PATH=${lib.makeBinPath (with pkgs; [ wl-clipboard playerctl fontconfig pipewire coreutils ])}"
          ];
        };
        Install.WantedBy = [ "graphical-session.target" ];
      };

      xdg.configFile."oxivox/config.toml" = lib.mkIf (cfg.settings != { }) {
        source = tomlFormat.generate "oxivox-config.toml" cfg.settings;
      };

      xdg.configFile."oxivox/theme.toml" = lib.mkIf (cfg.theme != null) {
        source = tomlFormat.generate "oxivox-theme.toml" cfg.theme;
      };
    }

    (lib.mkIf cfg.hyprland.enable {
      services.oxivox.settings.cancel_submap = lib.mkDefault "oxivox";

      wayland.windowManager.hyprland = {
        settings.bind = [ "${cfg.hyprland.toggleBind}, exec, oxivox toggle" ];
        # Active only while oxivox records or transcribes (the daemon switches in and out): Escape cancels, and the
        # toggle key must still work to stop a recording.
        submaps.oxivox.settings.bind = [
          ", ESCAPE, exec, oxivox cancel"
          "${cfg.hyprland.toggleBind}, exec, oxivox toggle"
        ];
      };
    })
  ]);
}
