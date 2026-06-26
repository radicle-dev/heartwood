{
  lib,
  buildGoModule,
  fetchFromGitHub,
}:
buildGoModule rec {
  pname = "dockerfile-json";
  version = "1.2.2";

  src = fetchFromGitHub {
    owner = "keilerkonzept";
    repo = "dockerfile-json";
    rev = "v${version}";
    hash = "sha256-B+MimQi0QaD+uToEB7LxZwtw4vowyjyLHGsD4fPuVIk=";
  };

  vendorHash = null;

  tags = ["dfrunsecurity"];

  # Mirror upstream's version stamp:
  ldflags = ["-X main.version=${version}"];

  meta = {
    description = "Parse and print a Dockerfile as JSON";
    homepage = "https://github.com/keilerkonzept/dockerfile-json";
    license = lib.licenses.mit;
    mainProgram = "dockerfile-json";
  };
}
