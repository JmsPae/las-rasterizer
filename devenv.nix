{
  pkgs,
  lib,
  config,
  inputs,
  ...
}: {
  packages = with pkgs; [time gdal];
  languages.rust = {
    channel = "stable";
    enable = true;
  };
}
