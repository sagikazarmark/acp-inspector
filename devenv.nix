{ pkgs, lib, ... }:

{
  dotenv.enable = true;

  dagger.enable = true;
  env.DAGGER_X_RELEASE = "v1.0.0-beta.11";

  packages =
    with pkgs;
    [
      # The two the documented workflow starts from, so the shell is whole on a
      # machine that has nothing else: `just` is the entry point (`just testy`,
      # `just test`, `just check`), and `scripts/build-testy.sh` clones and
      # fetches the rust-sdk checkout with `git`.
      just
      git

      lld

      cargo-audit
      cargo-deny
      cargo-dist
      cargo-release
      cargo-watch

      # `dx build --release` post-processes the wasm with wasm-opt.
      binaryen
      dioxus-cli
      wasm-pack
      wasm-bindgen-cli_0_2_126
    ]
    # The app's desktop shell (`crates/app`, its `desktop` feature) is a WebView
    # app: Dioxus desktop builds on wry/tao, which link the platform's browser
    # engine. On Linux that is WebKitGTK and its GTK/GLib/libsoup stack, found
    # through pkg-config at build time. The `web` feature needs none of it, so
    # these are here for the one renderer.
    #
    # Linux-only in both directions. None of it is in the shell's dependency
    # tree on macOS — there wry links WKWebView from the Apple SDK, which the
    # darwin stdenv already puts on every build — and it could not be there
    # anyway: `xdotool` is not packaged for darwin and `webkitgtk_4_1` is marked
    # broken on it, so an unguarded list stops `devenv shell` from evaluating on
    # a Mac. macOS therefore needs no counterpart list; the SDK is the whole of
    # it.
    ++ lib.optionals stdenv.hostPlatform.isLinux [
      pkg-config
      glib
      gtk3
      libsoup_3
      webkitgtk_4_1
      # Not ours, and both unconditional in `dioxus-desktop`'s Linux dependency
      # tree: tungstenite/native-tls for its hot-reload socket, which is OpenSSL
      # here (macOS resolves it to Security instead), and libxdo (in `xdotool`)
      # by way of its menu/hotkey stack.
      openssl
      xdotool
    ];

  languages = {
    rust = {
      enable = true;
      channel = "stable";
      targets = [ "wasm32-unknown-unknown" ];
    };
    javascript = {
      enable = true;
      npm.enable = true;
    };
  };

  # `devenv test` is the whole suite from a cold machine: build Testy, then
  # `cargo test`. The justfile's default recipe is already that pair, so this
  # defers to it rather than restating it.
  enterTest = ''
    just
  '';
}
