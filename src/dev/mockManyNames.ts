/**
 * What `?state=many` installs on top of the pretend Mac (docs/ui-preview.md,
 * `addMany` in ./mockData.ts): real tools, by the names their sources give
 * them, so that the rows show the logos and the lines the app has for them.
 * Every name is one the logo pack lists (the `tools` of
 * src/assets/tool-icons/pack.json) and the description tables have a line
 * for (the files in src/assets/tool-descriptions: the Chinese one for
 * Homebrew's, both for npm's, PyPI's and Cargo's), none is one the pretend
 * Mac already has, and each list is a sample of those, written out here so
 * that the scenario stays the same when the pack is rebuilt. Dev-only data,
 * like everything in src/dev.
 */

/** Names written as one run of whitespace-separated words. */
function words(text: string): readonly string[] {
  return text.trim().split(/\s+/);
}

/** Homebrew formulae the user asked for. */
export const MANY_FORMULAE = words(`
  ack agent-browser akamai amass angular-cli ansible ansible-lint apache-arrow apache-spark apko
  appium aqua arduino-cli argo argocd aria2 asciidoctor asciinema assimp astro atlas atuin
  autoconf automake avrdude aws-cdk aws-iam-authenticator aws-sam-cli aws-vault awscli azcopy
  azure-cli b2-tools bash bat bats-core bazel bazelisk bc bettercap binaryen binutils binwalk
  biome bison bitrise bitwarden-cli black borgbackup bruno-cli bubblewrap buf buildifier buildkit
  bun bundletool caddy capnp capstone carapace cargo-nextest carthage cask catch2 ccache ccusage
  certbot cfn-lint cgal check-jsonschema chromaprint cilium-cli circleci clamav clang-format
  cliproxyapi clojure cloud-sql-proxy cloudflared cmake cmake-docs cmctl cocogitto code-server
  coder commitizen composer conan conftest container cookiecutter copa coreutils cosign cppcheck
  crane crystal cups cycode cython czmq d2 dagger dart-sdk ddrescue delve deno dependency-check
  diff-so-fancy diffutils direnv docker-buildx docker-completion docker-compose
  docker-credential-helper docker-credential-helper-ecr docker-engine dolt doppler doxygen duckdb
  dvc edencommon eksctl elixir emscripten envoy erlang eslint etcd ethereum exercism eza fastapi
  fastfetch fastlane fb303 fbthrift ffmpeg-full ffuf findutils firebase-cli fizz flyctl flyway
  fontforge fonttools freerdp gambit-scheme gawk gdal gdb ggshield ghidra ghostscript
  gimme-aws-creds git-gui git-lfs git-svn gitui gitversion gleam glow glslang gnu-tar gnu-time
  gnuradio go go-task gogcli golangci-lint googletest googleworkspace-cli goose gopls goreleaser
  gpatch gradle grafana grafana-alloy grep groff grpc grpcui grype gsasl gsl gstreamer gtk+3 gtk4
  guile gum gzip hadolint handbrake harper hashcat haskell-stack hasura-cli hcloud hdf5 helix
  hello helm hermes-agent hf himalaya httpie hugo hurl ideviceinstaller imagemagick
  imagemagick-full inetutils influxdb infracost iperf3 ipopt ipython istioctl jackett jenkins
  jfrog-cli jj jsonnet julia jupyterlab just k3d k6 kimi-cli kimi-code kind kotlin krew
  kube-linter kubecolor kubernetes-cli kubescape kubeseal kustomize kyverno lastpass-cli lefthook
  lima lima-additional-guestagents linux-headers livekit livekit-cli llama.cpp lld llvm locust
  logcli lsd lua luajit luarocks lychee m4 macvim magic-wormhole make makensis mame mariadb
  mariadb-connector-c marp-cli mas maven mdbtools media-info mediamtx meilisearch memcached
  mermaid-cli meson micro micromamba micropython midnight-commander mihomo mingw-w64 minikube
  minio minio-mc mistral-vibe mit-scheme mlx mlx-lm mockery mockolo molten-vk mongodb-atlas-cli
  mongosh mpd mpv mruby mvfst mypy mysql mysql-client nasm nats-server neo4j neomutt neovim
  netdata netlify-cli newrelic-cli nginx nim node_exporter nuclei nuget numpy nvm nx oci-cli
  octave odin onnxruntime opa opam open-mpi openapi-generator openbao opencode opencv openjdk
  openshift-cli opentofu openvino openvpn oras osmium-tool osv-scanner otel-cli pandoc pdal pdfcpu
  pdm percona-toolkit periphery perl php phpmyadmin phpstan phrase-cli pi-coding-agent
  pinentry-mac plantuml platformio pmix pnpm pocketbase podman podman-compose poetry portaudio
  powershell pre-commit prettier proj prometheus protoc-gen-go-grpc prowler pulumi pup pydantic
  pygments pyright pytest python-setuptools python-tk pytorch qemu qpdf qsv qt qwen-code r
  rabbitmq radare2 railway ranger raylib rbenv rclone redis render renovate rom-tools root rpm rtk
  ruby ruby-build ruff runme rust rust-analyzer rustls-ffi rustup s2n saml2aws sbt scala sccache
  scipy scrcpy screen scw sdl2-compat sdl2_mixer sdl2_ttf sdl3 selenium-server semgrep sevenzip
  sfml sherlock sing-box snowflake-cli snyk-cli solana solidity sonar-scanner sops spaceship
  specify spicetify-cli sqlcipher sqlcmd squid srt ssh-copy-id starship step stern stow streamlink
  stripe-cli subfinder subversion supabase swi-prolog swift swift-protobuf swiftgen swiftlint swig
  syft syncthing tailscale tailwindcss talisman talosctl tcpdump tealdeer tectonic telegraf
  teleport telnet temporal tenv terraform-docs terraform-ls terragrunt tesseract tesseract-lang
  texinfo texlive tfsec tgenv tiger-vnc tilt tldr tlrc tmux tombi transmission-cli trivy
  trufflehog ty typescript typst ugrep unar unbound upx uutils-findutils uv v2ray v8 valkey velero
  vercel verilator vim virt-manager vite volta vte3 vulkan-headers vulkan-loader wakatime-cli
  wangle wasmtime watchexec watchman weechat wireguard-tools wireshark wp-cli wxwidgets xan
  xctesthtmlreport xonsh xorriso xray yaml-language-server yamlfmt yara yarn yasm ykman yosys
  yt-dlp z3 zellij zenity zeroclaw zig zizmor zola zsh zsh-autosuggestions zsh-completions
  zsh-syntax-highlighting
`);

/** Homebrew formulae it installed for them: libraries, folded behind "N components". */
export const MANY_DEPENDENCIES = words(`
  abseil boost brotli cairo fmt folly gd gdk-pixbuf gnutls harfbuzz jpeg-turbo jpeg-xl libadwaita
  libavif libb2 libfido2 libgit2 libiconv libidn2 libimobiledevice libmicrohttpd libomp libpcap
  libpq librdkafka librsvg libtool libusbmuxd libvpx libwebsockets libx11 libyaml mbedtls openjpeg
  p11-kit simdjson tbb utf8proc webp x265
`);

/**
 * Homebrew casks: the token, the name its cask gives it, and -- for one
 * that puts an app in /Applications -- the app's path there, which the
 * preview draws an icon for (./mockIcons.ts), and whether that app updates
 * itself, which leaves its update out of Homebrew's check until Settings'
 * Show self-updating apps is on.
 */
export const MANY_CASKS: ReadonlyArray<{ token: string; name: string; app?: string; autoUpdates?: true }> = [
  { token: "1password-cli", name: "1Password CLI" },
  { token: "adguard", name: "AdGuard", app: "AdGuard.app", autoUpdates: true },
  { token: "amneziavpn", name: "AmneziaVPN", app: "AmneziaVPN.app" },
  { token: "anaconda", name: "Anaconda Distribution" },
  { token: "android-commandlinetools", name: "Android SDK Command-line Tools" },
  { token: "android-ndk", name: "Android NDK" },
  { token: "antigravity-cli", name: "Antigravity CLI" },
  { token: "autodesk-fusion", name: "Autodesk Fusion", app: "Autodesk Fusion.app", autoUpdates: true },
  { token: "aws-vault-binary", name: "aws-vault" },
  { token: "battle-net", name: "Battle.net", app: "Battle.net.app", autoUpdates: true },
  { token: "citrix-workspace", name: "Citrix Workspace", app: "Citrix Workspace.app" },
  { token: "claude-code", name: "Claude Code" },
  { token: "clickhouse", name: "ClickHouse" },
  { token: "cloudflare-warp", name: "Cloudflare WARP", app: "Cloudflare WARP.app", autoUpdates: true },
  { token: "codeql", name: "CodeQL" },
  { token: "coderabbit", name: "CodeRabbit CLI" },
  { token: "codex", name: "Codex" },
  { token: "copilot-cli", name: "GitHub Copilot CLI" },
  { token: "cursor-cli", name: "Cursor CLI" },
  { token: "dotnet-runtime", name: ".NET Runtime" },
  { token: "dotnet-sdk", name: ".NET SDK" },
  { token: "dotnet-sdk@9", name: ".NET SDK 9" },
  { token: "elgato-stream-deck", name: "Elgato Stream Deck", app: "Elgato Stream Deck.app" },
  { token: "flutter", name: "Flutter SDK" },
  { token: "gcc-arm-embedded", name: "GCC ARM Embedded" },
  { token: "gcloud-cli", name: "Google Cloud CLI" },
  { token: "git-credential-manager", name: "Git Credential Manager" },
  { token: "gitkraken-cli", name: "GitKraken CLI" },
  { token: "google-drive", name: "Google Drive", app: "Google Drive.app", autoUpdates: true },
  { token: "google-japanese-ime", name: "Google Japanese Input" },
  { token: "gstreamer-development", name: "GStreamer Development" },
  { token: "gstreamer-runtime", name: "GStreamer Runtime" },
  { token: "karabiner-elements", name: "Karabiner-Elements", app: "Karabiner-Elements.app", autoUpdates: true },
  { token: "libreoffice-language-pack", name: "LibreOffice Language Pack" },
  { token: "macfuse", name: "macFUSE" },
  { token: "malwarebytes", name: "Malwarebytes for Mac", app: "Malwarebytes.app", autoUpdates: true },
  { token: "metasploit", name: "Metasploit Framework" },
  { token: "miniconda", name: "Miniconda" },
  { token: "miniforge", name: "Miniforge" },
  { token: "mullvad-vpn", name: "Mullvad VPN", app: "Mullvad VPN.app", autoUpdates: true },
  { token: "multipass", name: "Multipass" },
  { token: "mysql-shell", name: "MySQL Shell" },
  { token: "nextcloud", name: "Nextcloud", app: "Nextcloud.app", autoUpdates: true },
  { token: "ngrok", name: "ngrok" },
  { token: "nordvpn", name: "NordVPN", app: "NordVPN.app", autoUpdates: true },
  { token: "opensc-app", name: "OpenSC" },
  { token: "openvpn-connect", name: "OpenVPN Connect", app: "OpenVPN Connect.app", autoUpdates: true },
  { token: "openwebstart", name: "OpenWebStart", app: "OpenWebStart.app" },
  { token: "powershell@preview", name: "PowerShell Preview" },
  { token: "quarto", name: "Quarto" },
  { token: "r-app", name: "R", app: "R.app" },
  { token: "salesforce-cli", name: "Salesforce CLI" },
  { token: "sf-symbols", name: "SF Symbols", app: "SF Symbols.app" },
  { token: "sfm", name: "sing-box", app: "SFM.app" },
  { token: "snowflake-snowsql", name: "SnowSQL" },
  { token: "squirrel-app", name: "Squirrel" },
  { token: "steamcmd", name: "SteamCMD" },
  { token: "tailscale-app", name: "Tailscale", app: "Tailscale.app", autoUpdates: true },
  { token: "teamviewer", name: "TeamViewer", app: "TeamViewer.app", autoUpdates: true },
  { token: "temurin", name: "Eclipse Temurin JDK" },
  { token: "temurin@17", name: "Eclipse Temurin 17" },
  { token: "temurin@21", name: "Eclipse Temurin 21" },
  { token: "thonny", name: "Thonny", app: "Thonny.app" },
  { token: "tuist", name: "Tuist" },
  { token: "vagrant", name: "Vagrant" },
  { token: "virtualbox", name: "Oracle VirtualBox", app: "VirtualBox.app" },
  { token: "wireshark-chmodbpf", name: "Wireshark ChmodBPF" },
  { token: "xquartz", name: "XQuartz", app: "Utilities/XQuartz.app", autoUpdates: true },
  { token: "zerotier-one", name: "ZeroTier One", app: "ZeroTier.app" },
  { token: "zoom", name: "Zoom", app: "zoom.us.app", autoUpdates: true },
];

/** npm global packages. */
export const MANY_NPM = words(`
  @aws-amplify/cli @cloudflare/wrangler @docusaurus/core @github/copilot
  @githubnext/github-copilot-cli @google/gemini-cli @kubb/cli @memlab/cli @mermaid-js/mermaid-cli
  @microsoft/rush @playwright/test @qwen-code/qwen-code @socketsecurity/socket-patch
  @vscode/test-cli @web/test-runner agent-cli-detector appcenter-cli auto bugsnag-build-reporter
  commitlint create-react-app docsify-cli html-minifier-terser i18next-cli lerna lint-staged
  mastra mint native-run netlify pa11y-ci pnpm semantic-release sequelize-cli supabase tap turbo
  workbox-cli wrangler xo
`);

/** pipx tools. */
export const MANY_PIPX = words(`
  ansible asciinema certbot csvkit detect-secrets fastapi-cli localstack meson ocrmypdf platformio
  rich-cli sqlfluff toolong
`);

/** uv tools. */
export const MANY_UV = words(`
  ansible-lint azure-cli commitizen dbt-core dvc jupyterlab markitdown mkdocs pdm pylint semgrep
  tldr
`);

/** Crates installed with `cargo install`. */
export const MANY_CARGO = words(`
  bat bpf-linker cargo-binutils cargo-cyclonedx cargo-public-api cargo-watch espflash eza
  flip-link hayagriva honggfuzz just magika mdbook-i18n-helpers release-plz sccache starship
  tokio-console wasm-shrink watchexec-cli
`);

/** Ollama models, each of a family the pack has a logo for, with its size on disk. */
export const MANY_MODELS: ReadonlyArray<{ name: string; sizeBytes: number }> = [
  { name: "deepseek-r1:14b", sizeBytes: 8_988_112_040 },
  { name: "gemma3:12b", sizeBytes: 8_149_190_253 },
  { name: "gpt-oss:20b", sizeBytes: 13_793_441_244 },
  { name: "mistral:7b", sizeBytes: 4_372_824_384 },
  { name: "nomic-embed-text:latest", sizeBytes: 274_302_450 },
  { name: "qwen3:8b", sizeBytes: 5_225_387_923 },
];
