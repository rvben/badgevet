{
  lib,
  stdenv,
  rustPlatform,
  installShellFiles,
}:

let
  manifest = lib.importTOML ../Cargo.toml;
in
rustPlatform.buildRustPackage (finalAttrs: {
  pname = manifest.package.name;
  version = manifest.package.version;
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../src
      ../tests
      ../schemas
      ../README.md
      ../LICENSE
    ];
  };
  cargoLock.lockFile = ../Cargo.lock;

  nativeBuildInputs = [ installShellFiles ];
  doCheck = stdenv.buildPlatform.canExecute stdenv.hostPlatform;

  postInstall = lib.optionalString (stdenv.buildPlatform.canExecute stdenv.hostPlatform) ''
    for shell in bash fish zsh; do
      "$out/bin/badgevet" completions "$shell" > "badgevet.$shell"
    done
    installShellCompletion badgevet.{bash,fish,zsh}
  '';

  doInstallCheck = stdenv.buildPlatform.canExecute stdenv.hostPlatform;
  installCheckPhase = ''
    runHook preInstallCheck
    test "$("$out/bin/badgevet" --version)" = "badgevet ${finalAttrs.version}"
    "$out/bin/badgevet" --help > /dev/null
    for completion in \
      "$out/share/bash-completion/completions/badgevet.bash" \
      "$out/share/fish/vendor_completions.d/badgevet.fish" \
      "$out/share/zsh/site-functions/_badgevet"; do
      if ! test -s "$completion"; then
        echo "Missing or empty completion file: $completion" >&2
        exit 1
      fi
    done
    runHook postInstallCheck
  '';

  meta = {
    inherit (manifest.package) description homepage;
    license = lib.licenses.mit;
    mainProgram = "badgevet";
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
      "aarch64-darwin"
    ];
  };
})
