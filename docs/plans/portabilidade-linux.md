# Plano: levar o Queijo ao máximo de distros Linux

Plano de execução para um agente (Sonnet 5.5). Leia tudo antes de começar.

## 0. Contexto que você precisa ter

**O problema não é a distro, é o ambiente gráfico.** O binário Rust já compila e roda
em qualquer distro glibc. O que limita o Queijo hoje são as integrações, todas
amarradas ao GNOME Shell:

| Função | Hoje | Onde |
| --- | --- | --- |
| Screenshot | D-Bus `org.gnome.Shell.Screenshot`, pegando emprestado o nome `org.gnome.Screenshot` para passar na allowlist; fallbacks `gnome-screenshot` e `grim` | `src/capture.rs` (`connect`, `shell_shot`, `capture_with_window`) |
| Gravação | `org.gnome.Shell.Screencast.ScreencastArea` | `src/record.rs` (`run`) |
| Modo janela | extensão do GNOME Shell (`ListWindows`/`CaptureWindow`), só GNOME 42 (sintaxe `imports`, pré-ESM) | `gnome-extension/`, `src/shell_ext.rs` |
| Indicador de gravação | extensão do GNOME Shell | idem |
| Atalhos globais | `gsettings` em `org.gnome.settings-daemon.plugins.media-keys` (já atrás de `trait Store`) | `src/shortcut.rs` |
| Monitores | Wayland `xdg-output`, ou `xrandr` em subprocesso | `src/monitors.rs` |
| Área de transferência | `wl-copy` / `xclip` | `src/output.rs` (`copy_data`) |
| Notificações | `notify-send` | `src/output.rs` (`notify_always`) |
| Seletor de pasta | `zenity` | `src/settings.rs` (`pick_folder`) |
| Overlay | uma viewport transparente por monitor, que escurece a tela ao vivo; `LIBGL_ALWAYS_SOFTWARE=1` em **todo** Wayland por causa de um bug do Mutter 42 | `src/main.rs`, `src/app.rs` |
| Pacotes | `.deb` para Ubuntu 22.04 (glibc 2.35), `.tar.gz`, zip da extensão | `Cargo.toml`, `scripts/build-packages.sh` |

Os ambientes que queremos cobrir, por ordem de prioridade:

1. **GNOME 42–49**, Wayland e X11 (Ubuntu, Fedora, Debian, Arch, openSUSE)
2. **KDE Plasma 5.27 e 6**, Wayland e X11 (Kubuntu, Fedora KDE, KDE neon, openSUSE, Arch)
3. **wlroots**: Sway e Hyprland (no Hyprland, verifique o que é igual e o que é diferente)
4. **X11 genérico**: XFCE, Cinnamon, MATE, i3/bspwm, com ou sem compositor
5. Qualquer outro Wayland que tenha `xdg-desktop-portal` (COSMIC, etc.), via portais

A estratégia é uma **cadeia de backends** por função. Cada uma tenta, em ordem, o
caminho nativo e rápido do ambiente detectado e cai para algo mais genérico
(portal → ferramenta externa). Assim o GNOME 42, que já funciona, continua no
mesmo caminho de hoje.

## 1. Regras de trabalho (obrigatórias)

- Trabalhe na branch `test`. **Um commit por passo** numerado deste plano. As mensagens
  seguem o estilo do repositório: inglês, imperativo, frase curta, sem prefixo
  (`Add a desktop detection module`). Termine cada mensagem com a linha de coautoria
  que o sistema indicar. **Não faça push, não abra PR e não mexa na versão** sem o
  usuário pedir.
- Antes de cada commit, os três têm que passar:
  `cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked`.
  Se mudar algum script de shell, rode também o `shellcheck`.
- **Não quebre o GNOME 42 Wayland**, que é o único ambiente validado. Em tudo que mudar,
  o caminho do GNOME tem que continuar sendo o primeiro da cadeia e fazer exatamente o que
  faz hoje.
- Escreva código como o código ao redor: comentários curtos, só onde explicam um
  *porquê*; funções pequenas; erros como `Result<_, String>`; `eprintln!("queijo: …")`
  para diagnósticos. Se o código for testável sem desktop, coloque os testes em
  `mod tests` no próprio arquivo, como os módulos existentes fazem.
- Todo texto que o usuário vê vai para `locales/en.toml` **e** `locales/pt-BR.toml`
  (ver `docs/TRANSLATING.md`). Os testes de `i18n.rs` checam as chaves.
- A cada mudança visível, adicione uma entrada em **Unreleased** no `CHANGELOG.md`.
- Dependências novas: prefira as que já estão no projeto (`zbus` 5, `wayland-client`,
  `serde`). Antes de adicionar um crate, confira com `cargo tree` que ele não puxa outra
  versão do `zbus` nem um runtime async pesado. Use `--locked` e commite o `Cargo.lock`.
- **Não afirme que algo funciona num ambiente que você não testou.** Você não tem KDE,
  Sway ou XFCE de verdade. Para cada backend novo, escreva os testes que dá para rodar
  (parsers, detecção, Xvfb, Sway headless) e registre a verificação manual que falta em
  `docs/TESTING-DESKTOPS.md` (você cria esse arquivo no passo 1.3), como item pendente.
- Se um passo travar (API restrita, comportamento do compositor que não dá para
  contornar), **pare naquele passo e reporte**: o que tentou, o que viu e as opções que
  existem. Não improvise uma solução frágil para fechar o passo.

## 2. Fase 1: Base (sem mudar o comportamento)

### 1.1 Detecção do ambiente: `src/desktop.rs`
- Crie `pub struct Env { session: Session, desktop: Desktop, compositing: bool }`, com
  `Session::{Wayland, X11}` e `Desktop::{Gnome { major: Option<u32> }, Kde { major: Option<u32> }, Sway, Hyprland, Xfce, Cinnamon, Mate, Other(String)}`.
- Fontes, nesta ordem: `XDG_CURRENT_DESKTOP` (lista separada por `:`), `XDG_SESSION_DESKTOP`,
  `DESKTOP_SESSION`, `SWAYSOCK`, `HYPRLAND_INSTANCE_SIGNATURE`, `XDG_SESSION_TYPE` e
  `WAYLAND_DISPLAY`/`DISPLAY`. A versão do GNOME vem de `gnome-shell --version`, a do KDE
  de `plasmashell --version` (rodadas em subprocesso, só quando preciso, com cache em `OnceLock`).
- Separe a função pura (`fn detect_from(vars: &dyn Fn(&str) -> Option<String>) -> Env`) da que
  lê o ambiente real, e teste a pura com tabelas de casos: `ubuntu:GNOME`, `KDE`,
  `sway`, `Hyprland`, `X-Cinnamon`, `XFCE`, `MATE`, `pop:GNOME`, vazio.
- Troque `monitors::is_wayland()` para usar o `Env`, mantendo a assinatura para não
  espalhar a mudança.

### 1.2 `queijo --doctor`
- Nova flag em `src/cli.rs`. Ela imprime o ambiente detectado, os monitores, qual backend
  cada função usaria e **por quê**, e quais ferramentas externas faltam (`ffmpeg`, `wl-copy`,
  `xclip`, `grim`, `gst-launch-1.0`…), com o comando de instalação adequado à distro.
  A distro vem de `/etc/os-release` (`ID`, `ID_LIKE`), e o gerenciador de pacotes pode ser
  `apt`, `dnf`, `zypper`, `pacman` ou `apk`.
- O parser do `os-release` e a montagem do comando de instalação têm testes unitários.
  Adicione também um teste de integração em `tests/cli.rs` que confere que `--doctor` termina
  com código 0.
- Isso vai ser a primeira coisa a pedir em relatos de bug. Mencione no README.

### 1.3 Matriz de testes manual
- Crie `docs/TESTING-DESKTOPS.md` com uma tabela de ambientes (linhas) × funções (colunas:
  overlay, screenshot, área, tela, janela, vídeo, GIF, clipboard, atalho, indicador). As
  células ficam `✅ testado (data, quem)`, `⏳ pendente` ou `❌ não suportado (motivo)`.
  Preencha o GNOME 42 Wayland com o que o README já afirma e deixe o resto como ⏳.
  Inclua as instruções para montar cada ambiente numa VM (imagem live da distro, GNOME Boxes ou virt-manager).

### 1.4 Refatorar a captura em cadeia, sem mudar o comportamento
- Em `src/capture.rs`, transforme a sequência fixa (Shell → `gnome-screenshot` → `grim`)
  numa lista de backends escolhida pelo `Env`. Algo como
  `fn screenshot_backends(env: &Env) -> Vec<Backend>` e um laço que tenta cada um, guarda o
  erro e passa ao próximo. A ordem para GNOME tem que sair idêntica à de hoje, e um
  teste unitário garante isso.
- A captura da janela focada e a `Shell` usada em `with_pointer` continuam sendo só do
  backend GNOME (`Option`).

## 3. Fase 2: Pequenas portabilidades de alto retorno

### 2.1 Notificações via D-Bus
- Troque o `notify-send` por `org.freedesktop.Notifications.Notify` via `zbus`. Use o hint
  `image-path` para o ícone, `expire_timeout` de 4000 e `app_name` `Queijo`. Respeite
  `QUEIJO_NO_NOTIFY`.
- Se o D-Bus falhar, use `notify-send` como fallback. Tire o `libnotify-bin` dos `depends`
  do `.deb` e passe para `recommends`.

### 2.2 Seletor de pasta
- Ordem: portal `org.freedesktop.portal.FileChooser.OpenFile` com `directory: true` →
  `zenity` → `kdialog --getexistingdirectory` → se nenhum funcionar, devolva `None` (o campo de
  texto já permite digitar). O portal responde por um sinal `Response` num objeto
  `Request`. Escreva um helper genérico `portal::request(...)` em `src/portal.rs`, porque ele
  será reutilizado nas fases 3 e 4. Assine o sinal **antes** de chamar o método (o handle
  do request é previsível: `/org/freedesktop/portal/desktop/request/<sender>/<token>`).
- O `zenity` passa de `depends` para `recommends`.

### 2.3 Área de transferência
- Mantenha `wl-copy`/`xclip` e acrescente `xsel --clipboard --input` como último fallback no X11 (só
  para `text/*`, já que o `xsel` não define o MIME; para imagem ele não serve, então nem tente).
  Mensagem de erro clara via i18n, dizendo qual pacote instalar.

### 2.4 `LIBGL_ALWAYS_SOFTWARE` só no Mutter
- Hoje esse workaround vale para todo Wayland (`src/main.rs`). Restrinja a `Desktop::Gnome`.
  Nos outros, deixe a renderização na GPU. Registre em `TESTING-DESKTOPS.md` que, se o overlay
  aparecer preto no KDE ou no wlroots, a correção é estender a condição.
- Adicione `QUEIJO_SOFTWARE_GL=0|1` para forçar o comportamento (útil para relatos de bug) e
  documente no README.

### 2.5 X11 sem compositor: modo congelado
- Sem compositor (sem dono da seleção `_NET_WM_CM_S0`), janelas transparentes ficam pretas.
  Detecte isso com `x11rb` (crate novo, puro Rust: `GetSelectionOwner`) e guarde em `Env.compositing`.
- Sem compositor, o editor tem que mostrar a captura congelada por baixo (comportamento da
  0.1.0) em vez de transparência. `QueijoApp::frozen_texture` (`src/app.rs:1212`) ainda existe e
  é usado em `screen_ui`; veja o commit `fb00ec4` (`git show fb00ec4 -- src/app.rs`) para
  entender o que mudou e como reativar o desenho do fundo congelado atrás de uma flag `frozen: bool` em `QueijoApp`.
  `clear_color` e a espera pela captura antes do primeiro frame precisam acompanhar.
- Teste no Xvfb (que não tem compositor): o smoke test deve abrir o editor em modo
  congelado e o fundo não pode ficar preto. Estenda `scripts/smoke-test.sh`.

### 2.6 Monitores no X11 sem `xrandr`
- Troque o subprocesso `xrandr --listmonitors` por RandR via `x11rb` (`get_monitors`). Mantenha
  `parse_xrandr` como fallback e os testes dele.
- Confira a ordem dos monitores contra o que o winit enumera, porque `Monitor.index` precisa
  bater com `ViewportBuilder::with_monitor`.

## 4. Fase 3: Screenshot em todo lugar

Cadeia por ambiente. A primeira entrada de cada linha é a preferida:

| Ambiente | Cadeia |
| --- | --- |
| GNOME | Shell D-Bus (atual) → portal Screenshot → `gnome-screenshot` |
| KDE | KWin `org.kde.KWin.ScreenShot2.CaptureWorkspace` → `spectacle -b -n -f -o` → portal |
| Sway/Hyprland/wlroots | `wlr-screencopy` nativo **ou** `grim` → portal |
| X11 (qualquer) | `x11rb` `GetImage` na janela raiz → `maim`/`import` → portal |
| Outro Wayland | portal → `grim` |

### 3.1 X11 nativo
- `GetImage` (ZPixmap) da raiz com `x11rb`, convertendo BGRx → RGBA. Leve em conta a profundidade e
  a ordem de bytes do servidor. Teste de integração no Xvfb: desenhe algo conhecido com
  `xsetroot -solid '#ff0000'` e confira os pixels.
- O ponteiro (para `with_pointer`) vem da extensão XFixes (`GetCursorImage`), composto por
  cima. Implemente isso, ou deixe como `None` e registre como limitação.

### 3.2 wlroots
- Comece pelo `grim` em subprocesso (já existe como fallback). Ele serve de backend principal para
  `Desktop::Sway | Hyprland`. Implementar `zwlr_screencopy_manager_v1` direto com
  `wayland-protocols-wlr` é opcional: só faça se o `grim` se mostrar lento (> 300 ms) no Sway
  headless.
- CI: crie um job que sobe `sway` headless (`WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 sway`)
  num contêiner `archlinux` ou `fedora`, roda `queijo --quick` e confere que um PNG do tamanho
  da saída foi salvo.

### 3.3 KDE
- A `ScreenShot2` do KWin é restrita: o KWin só atende apps cujo `.desktop` declara
  `X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2`. Adicione essa chave a
  `packaging/queijo.desktop` (o `desktop-file-validate` aceita chaves `X-`) e verifique como o
  KWin associa o processo ao `.desktop` (pelo caminho do executável). A interface devolve os dados
  num **pipe** (um file descriptor que você passa), não num arquivo; leia os metadados devolvidos
  (largura, altura, stride, formato) para decodificar. Prefira `CaptureWorkspace` e
  `CaptureActiveWindow` para a janela focada.
- Se a restrição não puder ser satisfeita (por exemplo, binário instalado fora do caminho do `.desktop`),
  use `spectacle` como fallback. **Spike primeiro**: se não conseguir confirmar a API pela
  documentação ou pelo código do KWin (`src/plugins/screenshot/` no repositório do KWin),
  implemente só `spectacle` e o portal e registre a pendência.

### 3.4 Portal Screenshot (fallback universal)
- `org.freedesktop.portal.Screenshot.Screenshot` com `interactive: false`, usando o helper `portal::request`.
  O portal devolve uma URI `file://`: leia e apague o arquivo se ele estiver em diretório temporário.
  No GNOME o portal pede permissão na primeira vez, e isso é aceitável como fallback.
- O portal não informa a geometria dos monitores. Use a de `monitors::detect()` para
  fazer o `split`; se as dimensões não baterem (escala fracionária), escale a imagem
  para o tamanho lógico total antes, e registre isso com um `eprintln!`.

### 3.5 Escala fracionária
- O `Monitor` está em coordenadas **lógicas** e a captura pode vir em pixels **físicos**. Escreva
  um teste com um desktop de 2 monitores, um a 1.0 e outro a 1.5, e garanta que `split`/`stitch`
  continuam corretos quando a imagem é maior que a soma lógica (o GNOME devolve físico).
  Se o código atual assumir 1:1, introduza `Monitor.scale` (do `wl_output::Event::Scale` /
  `xdg_output` logical size vs mode) e use-o no recorte. Não mude o resultado no GNOME 1.0×.

## 5. Fase 4: Gravação em todo lugar

`record::run` hoje faz tudo com o GNOME. Extraia um `trait Recorder { fn start(&self, req, path) -> Result<Handle, String>; }`
(ou um enum). O `Handle` para a gravação e devolve o arquivo final. Mantenha a lógica de parada
(socket + D-Bus `Recorder`) e a conversão para GIF iguais para todos.

| Ambiente | Backend |
| --- | --- |
| GNOME | Shell Screencast (atual) |
| X11 | `ffmpeg -f x11grab -video_size WxH -framerate N -draw_mouse 0/1 -i :0.0+X,Y` |
| Sway/Hyprland | `wf-recorder -g "X,Y WxH"` (ou `wl-screenrec`), parado com `SIGINT` |
| KDE e outros Wayland | portal ScreenCast + PipeWire via `gst-launch-1.0 pipewiresrc` |

### 4.1 X11 com ffmpeg
- Vídeo: WebM VP8/VP9 (`-c:v libvpx-vp9 -deadline realtime -cpu-used 8`, ou VP8 se o VP9 ficar lento).
  GIF: grave sem perdas (`-c:v ffv1` em `.mkv`) e reutilize `gif::convert`. Pare escrevendo `q`
  no stdin do ffmpeg (não use kill, senão o arquivo fica corrompido).
- Teste de integração no Xvfb: grave 1 s de uma área e confira com `ffprobe` que o arquivo tem
  as dimensões pedidas (pares).

### 4.2 wlroots
- `wf-recorder` em subprocesso, pare com `SIGINT` e espere o processo terminar. O ponteiro é
  controlado por `--no-cursor`/padrão, conforme a versão; confira com `wf-recorder --help`.
  Para GIF, use `-c ffv1 -m matroska`.
- Teste no job do Sway headless.

### 4.3 Portal ScreenCast + PipeWire
- Fluxo: `CreateSession` → `SelectSources` (`types: MONITOR`, `cursor_mode` conforme `req.pointer`,
  `persist_mode: 2`) → `Start` → `OpenPipeWireRemote` (devolve um fd). Guarde o `restore_token`
  em `config.toml` (campo novo, com `#[serde(default)]`) para não perguntar de novo.
- O usuário escolhe o monitor no diálogo do portal. A área pedida é recortada com
  `videocrop` no pipeline do GStreamer, com base na posição do monitor escolhido (o portal
  devolve `position`/`size` do stream). Se a área cruzar monitores, grave só o monitor
  que contém a maior parte e avise via notificação.
- Rode `gst-launch-1.0 pipewiresrc fd=3 path=<node> ! videoconvert ! videocrop ... ! vp8enc ! webmmux ! filesink`
  passando o fd herdado como fd 3 (`CommandExt::pre_exec` com `dup2`, ou `std::os::fd`). Pare com
  `SIGINT` e `-e` (EOS) para fechar o arquivo direito.
- O diálogo do portal aparece *depois* que o editor fecha. Isso é aceitável, mas a contagem regressiva
  do editor deixa de fazer sentido: nesse backend, mostre a contagem só depois do `Start`
  (via notificação), ou pule a contagem. Decida, implemente e documente a escolha.
- Dependências de pacote: `gstreamer1.0-pipewire` / `gstreamer1-plugin-pipewire`,
  `gstreamer1.0-plugins-good`, como `recommends`.

### 4.4 Parada da gravação
- O socket de instância única já funciona em qualquer distro (é Linux puro). Garanta que
  `queijo --stop` e o atalho funcionam com todos os backends.

## 6. Fase 5: Modo janela fora do GNOME

Introduza `trait WindowSource { fn list(&self) -> Option<Vec<WindowInfo>>; fn capture(&self, &WindowInfo) -> Option<RgbaImage>; }`
e deixe a extensão como uma implementação. Quando não houver imagem própria da janela, a
miniatura vem recortada da captura do desktop (só a parte visível), e isso é aceitável.

- **X11**: `_NET_CLIENT_LIST_STACKING` + `_NET_WM_NAME` + `WM_CLASS` + `_NET_FRAME_EXTENTS` +
  `TranslateCoordinates` via `x11rb`. Foco por `_NET_ACTIVE_WINDOW`. Ignore `_NET_WM_WINDOW_TYPE_DOCK/DESKTOP`
  e a própria janela do Queijo. Imagem: `GetImage` da janela, que só funciona se ela estiver visível;
  senão, recorte do desktop. Teste no Xvfb com duas janelas `xterm`/`xmessage`.
- **Sway**: `swaymsg -t get_tree -r` (JSON); percorra os nós com `pid`/`app_id`/`rect`/`visible`.
  **Hyprland**: `hyprctl -j clients` + `hyprctl -j activewindow`. Adicione o `serde_json`. Os parsers
  ficam em funções puras, testadas com JSON de exemplo salvo em `tests/fixtures/`.
- **KDE**: spike. Opções: carregar um KWin script via `org.kde.KWin /Scripting loadScript`
  que devolva a lista por D-Bus, ou `CaptureWindow` da ScreenShot2 com `CaptureInteractive`.
  Se não houver um caminho limpo, o KDE fica só com a janela ativa (`CaptureActiveWindow`) e você
  registra a pendência.
- **Sem fonte**: comportamento de hoje (a janela focada, achada por correspondência de imagem).

## 7. Fase 6: GNOME além do 42

### 6.1 Extensão para GNOME 45+
- O GNOME 45 trocou o formato das extensões para ES modules (`import`/`export default class extends Extension`).
  Mantenha `gnome-extension/queijo@queijo.app/` (42–44, `shell-version: ["42","43","44"]`) e crie
  `gnome-extension/esm/queijo@queijo.app/` (45–49). Extraia a lógica comum quando der; quando não der,
  duplique e comente que os dois arquivos devem andar juntos.
- `build-packages.sh` gera dois zips (`-gnome42.zip`, `-gnome45.zip`). `install.sh` e o `.deb`
  instalam a variante certa conforme `gnome-shell --version` (o `.deb` pode instalar as duas em
  pastas separadas e um `postinst` cria o link, ou o próprio binário instala a certa na primeira execução;
  escolha a mais simples e explique).
- CI: `node --check` nas duas, e a verificação de metadata cobrindo as duas listas de versões.

### 6.2 APIs restritas em GNOME mais novo
- O `connect()` pega emprestado o nome `org.gnome.Screenshot` para passar na allowlist do Shell.
  Verifique no código do gnome-shell (`js/ui/screenshot.js` e `js/ui/screencast.js`, tags 43…49)
  se a allowlist, ou o `ScreencastArea`, mudou. Se em alguma versão a chamada for recusada,
  a cadeia já cai para o portal (fases 3 e 4). Garanta que a mensagem de erro aparece no `--doctor`.
- Alternativa robusta para GNOME 45+: a extensão (que roda dentro do Shell e não tem restrição)
  pode oferecer `Screenshot`/`ScreencastArea` próprios. Implemente isso só se o 6.2 mostrar que é necessário.

### 6.3 Atalhos em outros ambientes
`src/shortcut.rs` já abstrai `trait Store`. Crie implementações por ambiente:
- **Cinnamon**: `org.cinnamon.desktop.keybindings` (`custom-list` + `custom-keybinding:/org/cinnamon/desktop/keybindings/custom-keybindings/customN/`).
- **MATE**: `org.mate.control-center.keybinding` em `/org/mate/desktop/keybindings/customN/`.
- **XFCE**: `xfconf-query -c xfce4-keyboard-shortcuts -p /commands/custom/<Accel> -n -t string -s 'queijo …'`.
  O formato da tecla é `<Primary><Shift>Print`; escreva a conversão e teste.
- **KDE**: spike. O caminho conhecido é um `.desktop` com `X-KDE-Shortcuts=` em
  `~/.local/share/kglobalaccel/` (Plasma 6) e um aviso para reiniciar a sessão; confirme na
  documentação do `kglobalaccel` antes de implementar.
- **Sway/Hyprland/i3/outros**: não escreva no arquivo de configuração do usuário. A tela de
  atalhos mostra a linha pronta (`bindsym Print exec queijo` / `bind = , Print, exec, queijo`)
  com um botão de copiar.
- Em todos os casos, a seção de atalhos das configurações diz qual mecanismo está em uso.

## 8. Fase 7: Indicador de gravação fora do GNOME

- Use o crate `ksni` (StatusNotifierItem, puro Rust, D-Bus) no processo `--record` para mostrar
  um ícone na bandeja com o tempo de gravação e a ação "Parar". Funciona em KDE, XFCE, Cinnamon,
  MATE, waybar e no GNOME com a extensão AppIndicator. Confira se o `ksni` usa uma versão do `zbus`
  compatível; se não usar, implemente o SNI mínimo direto com o `zbus` 5.
- No GNOME com a extensão do Queijo ativa, não mostre o SNI (para não duplicar). Detecte isso pelo
  `list_windows` respondendo.

## 9. Fase 8: Pacotes e CI para muitas distros

### 8.1 Baseline de glibc
- Compilando no Ubuntu 22.04, o binário exige glibc 2.35 e não roda no Debian 11 nem no RHEL/Alma/Rocky 8–9
  (2.28/2.34). Use `cargo-zigbuild` com o target `x86_64-unknown-linux-gnu.2.28`. Tudo o que é
  gráfico já é carregado em tempo de execução (`dlopen`), então não há bibliotecas para linkar.
  Confira com `objdump -T target/.../queijo | grep GLIBC_ | sort -V | tail -1`.
- Adicione `aarch64-unknown-linux-gnu.2.28` também.

### 8.2 Formatos
| Formato | Alvo | Ferramenta |
| --- | --- | --- |
| `.deb` | Debian 12+, Ubuntu 22.04+, Mint, Pop!_OS | `cargo-deb` (atual), deps revisadas: o que não for essencial vai para `recommends` |
| `.rpm` | Fedora, RHEL/Alma/Rocky 9, openSUSE | `cargo-generate-rpm`; dependências com *rich deps*: `(ffmpeg-free or ffmpeg)`, `wl-clipboard` |
| AUR | Arch, Manjaro, EndeavourOS | `packaging/aur/PKGBUILD` (`queijo-bin`, baixando o `.tar.gz` do release) + `.SRCINFO` |
| `.tar.gz` | qualquer distro | atual; `install.sh` passa a imprimir o comando de dependências do `--doctor` |
| AppImage | qualquer distro | opcional, e só se for simples: as ferramentas externas continuam sendo do sistema |

- Os nomes de pacote das dependências mudam por distro. Monte uma tabela única em
  `packaging/dependencies.md`, que alimenta o `--doctor`, o README e os manifestos.
  Use o mesmo dado no código (uma tabela `const` em `src/doctor.rs`) para não divergir.

### 8.3 CI
- Novo job `distros` em matriz de contêineres: `debian:12`, `ubuntu:24.04`, `fedora:latest`,
  `opensuse/tumbleweed`, `archlinux:latest`. Em cada um: instale o pacote gerado com o gerenciador
  nativo (`apt install ./*.deb`, `dnf install ./*.rpm`, `zypper install --allow-unsigned-rpm`, `pacman -U`
  a partir do PKGBUILD) e rode `queijo --help` e `queijo --doctor`. É isso que prova que as dependências
  declaradas existem com esses nomes.
- Rode também o binário 2.28 dentro de `rockylinux:8` (`--help` e `--doctor`).
- O job de Sway headless (3.2/4.2) e os testes de Xvfb (2.5, 3.1, 4.1, 5) entram no `ci.yml`.
- `release.yml`: publique `.rpm`, os zips da extensão e o PKGBUILD junto com o resto.
  Atualize `docs/RELEASING.md`.

### 8.4 Documentação
- README e README.pt-BR: troque a seção *Compatibility* por uma tabela ambiente × função
  gerada do `TESTING-DESKTOPS.md` (só ✅ conta como suportado; ⏳ aparece como "deve funcionar,
  não testado"). Instruções de instalação por distro. Remova a frase "Outside GNOME there is no
  screen recording" quando a fase 4 entrar.

## 10. Fase 9 (opcional, depois de tudo): Flatpak/Flathub

Dentro do sandbox do Flatpak, o D-Bus do GNOME Shell, o empréstimo de nome, `gsettings` do host
e as ferramentas externas não estão disponíveis. Só os portais funcionam. Ou seja, o Flatpak só fica viável
depois das fases 3.4 e 4.3, usando **apenas** os backends de portal mais o `ffmpeg` empacotado.
Se o usuário pedir esta fase, escreva primeiro um documento de viabilidade
(o que funcionaria e o que se perde) e espere aprovação antes de criar o manifesto.

## 11. Ordem de entrega e pontos de parada

Cada fase termina com tudo verde e pode virar um release por si só. **Ao fim de cada fase, pare
e resuma ao usuário** o que foi feito, o que foi testado de verdade, o que ficou ⏳ e o que você
recomenda testar numa VM antes de seguir. Só continue com o "ok" dele.

1. Fase 1 (base): sem mudança visível
2. Fase 2 (portabilidades pequenas): já melhora XFCE/Cinnamon/MATE/i3
3. Fase 3 (screenshots): screenshot funcionando em KDE, wlroots e X11
4. Fase 4 (gravação)
5. Fase 5 (janelas)
6. Fase 6 (GNOME novo + atalhos)
7. Fase 7 (bandeja)
8. Fase 8 (pacotes + CI): pode ser antecipada em parte (8.1 e `.rpm`) se o usuário quiser
   distribuir antes
9. Fase 9: só se pedida

## 12. Critério de pronto do plano todo

- `queijo --doctor` explica qualquer ambiente.
- Screenshot (área/tela/janela, copiar/salvar) e gravação (vídeo/GIF) têm um backend em cada um
  dos cinco grupos de ambiente do §0, com teste automatizado onde dá (Xvfb, Sway headless) e
  verificação manual registrada no resto.
- Pacotes instaláveis e testados em CI para Debian, Ubuntu, Fedora, openSUSE, Arch e EL8+.
- GNOME 42 Wayland continua se comportando exatamente como na 0.2.0.
