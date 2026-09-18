# Codenotch / ai-hud for Windows

Uma evolução e porte nativo para Windows do [Codenotch](https://github.com/vinzdg/codenotch) — o HUD minimalista que repousa na borda da tela e responde instantaneamente:
**quanto da sua cota de IA ainda resta**, e **se seus assistentes ainda estão gerando código**.

Construído em **Rust + Tauri v2 + Edge WebView2**, reproduzindo a mesma linguagem visual de anéis coloridos com gradientes, cards de hover com barras por janela temporal e suporte expandido a **8 provedores de IA**.

---

## 🚀 Comparativo: ai-hud vs. Codenotch Original (Upstream)

| Recurso / Funcionalidade | 🚀 ai-hud (Esta Versão) | 🏛️ Versão Original Upstream | Benefício / Impacto |
|---|:---:|:---:|---|
| **Provedores de IA** | **8 Provedores**<br>*(Claude, Codex, Cursor, Antigravity, DeepSeek, Meta Muse, OpenCode Go, MiniMax)* | **4 Provedores**<br>*(Claude, Codex, Cursor, Antigravity)* | Abrange praticamente todo o ecossistema moderno de codificação com IA. |
| **Integração MiniMax Token Plan** | ✅ **Oficial com Link Direto**<br>*(Intervalo 5h, cota semanal e botão para recarga no console)* | ❌ Não suportado | Acompanhe a queima de tokens do MiniMax com atalho para o console. |
| **Gerenciador Visual de Chaves de API** | ✅ **UI Gráfica com Máscara**<br>*(Exibição protegida `skmini-********1234` com botão `👁️`)* | ❌ Arquivos de texto manuais | Gerencie credenciais na tela de configurações com segurança visual. |
| **Controle de Opacidade Dinâmico** | ✅ **Transparência em Repouso (100% a 25%)**<br>*(Slider na config + botão rápido no topo do HUD)* | ❌ Opacidade fixa em 100% | O HUD fica translúcido para não tampar código, acendendo com o mouse. |
| **Atalhos Rápidos no HUD (Pill)** | ✅ **Botão de Opacidade + Engrenagem ⚙️** | ❌ Apenas menu de contexto no Tray | Alterne a transparência ou abra as configurações em 1 clique direto no HUD. |
| **Estabilidade & Zero-Lag (Windows 11)** | ✅ **Janela Pré-Alocada Anti-Deadlock**<br>*(Ciclo `hide/show` sem bloqueio da UI thread / COM)* | ⚠️ Risco de travamento (AppHangB1) ao recriar WebView2 em tempo de execução | Abertura instantânea (0ms), sem telas brancas e sem travar o Windows. |
| **Localização em Português (PT-BR)** | ✅ **100% Traduzido em PT-BR**<br>*(Com auto-detecção do idioma do sistema operacional)* | ❌ Apenas Inglês | Nativamente acessível para desenvolvedores brasileiros. |
| **Instalador Automatizado (NSIS)** | ✅ **`Codenotch-Setup.exe` via CI/CD**<br>*(Instalação per-user sem exigir admin)* | ⚠️ Build manual | Setup pronto e leve para Windows 10 e Windows 11. |

---

## Provedores Suportados (What it shows)

| Cell | Source | How it reads it |
|---|---|---|
| **Claude** | `GET https://api.anthropic.com/api/oauth/usage` with the token Claude Code keeps in `~/.claude/.credentials.json` | Session / weekly windows, 429 back-off with a persisted deadline, stale readings dimmed with their age. Renews that token by running the standalone `claude -p` shortly before it expires (Claude Code inside the desktop app never writes this file), and never sends an expired one. A thin arc spins inside the ring while a Claude session is working, and pulses amber when one is waiting on you (Claude Code hooks + transcript watcher, desktop app included). |
| **Codex** | The local Codex sign-in in `~/.codex/auth.json` (read only, never refreshed), falling back to the newest session snapshot | Live primary/secondary windows (5h + weekly on paid plans, a monthly window on free) while Codex is signed in; Spark and Code review appear on the hover card when Codex reports them; otherwise the last snapshot, marked stale by its own timestamp. |
| **Cursor** | The editor's own session from `state.vscdb` → `cursor.com/api/usage-summary` | Included usage / API usage / on-demand, reset at billing-cycle end. Nothing to sign into: it borrows the editor's session, so there is only ever one account. |
| **Antigravity** | Official `agy` CLI `/usage` print when installed; otherwise the existing local `language_server` bridge, Google Cloud Code API, or transcript model count | Official four quota rows (Gemini & Claude/GPT 5h/weekly) without running the full IDE. When CLI is absent, falls back to legacy local bridge/API. |
| **DeepSeek** | `DEEPSEEK_API_KEY` or `%APPDATA%\codenotch\deepseek.key` | Balance and usage inquiry via official DeepSeek API endpoints. |
| **Meta Muse** | `~/.config/muse/auth.json` and `settings.json` | Real-time session and model telemetry without arbitrary mock values. |
| **OpenCode Go**| `OPENCODE_API_KEY` or `%APPDATA%\codenotch\opencode.key` | Usage and allowance monitoring via OpenCode API endpoints. |
| **MiniMax** | `MINIMAX_API_KEY`, `%APPDATA%\codenotch\minimax.key` or `~/.config/minimax/key.txt` | Interval (5h) and weekly quota tracking via official Token Plan API (`api.minimax.io/v1/token_plan/remains`). |

Providers that are not installed simply do not get a cell.

### Antigravity

- **Official CLI (Preferred)**: When the official Antigravity CLI (`agy.exe`) is installed (`%LOCALAPPDATA%\agy\bin\agy.exe` or on `PATH`) and signed in, Codenotch reads official quotas directly without keeping the full IDE running.
- **Execution**: Runs the official CLI in a hidden Windows pseudo-console, with a 70-second timeout and cleanup of its process tree. It does not need PowerShell scripts or a separate service.
- **Refresh**: Checks at startup and on hover/explicit request when readings are at least five minutes old; failed attempts are also limited to once per five minutes. It keeps previous readings on failure, without switching to legacy APIs. The CLI is not launched periodically while idle.
- **Fallback**: When the official CLI is not installed, Codenotch preserves the legacy local bridge (`language_server`), Credential Manager, and transcript model turn counting to maintain compatibility with existing installations.
- **Official CLI Reference**: Standalone `/usage` printing is described in the [official Antigravity CLI documentation](https://www.antigravity.google/docs/cli/headless). Note: no categorical Terms of Service guarantee is made.

Restart Codenotch after installing or removing `agy`: the source is selected at startup.
The CLI's text report is parsed defensively; an unsupported format or failed sign-in
shows an error or the last reading marked stale. Codenotch does not automate sign-in.

## Install / build

Download [`Codenotch-Setup.exe`](https://github.com/vinzdg/codenotch/releases/latest/download/Codenotch-Setup.exe)
from the latest release. It installs for the current user without administrator rights, puts
`codenotch-hook.exe` beside the app where **Install hooks** looks for it, and fetches WebView2 if
Windows does not already have it. The installer is not code-signed, so SmartScreen stops it the
first time with *Windows protected your PC*: choose **More info**, then **Run anyway**.

To build from source instead — prerequisites: Rust (MSVC toolchain), WebView2 runtime (ships with Windows 11).

```powershell
# from this directory (the repo root here; `windows/` inside the upstream repo)
cargo build --release
.\target\release\codenotch.exe          # pill appears on the right edge of the primary monitor
.\target\release\codenotch.exe doctor   # self-diagnosis: credentials, data sources, icons, hooks
```

To build the installer the way the Windows Package workflow does:

```powershell
# the hook gets its own target dir, so the bundler never copies it onto itself
cargo build --release --locked -p codenotch-hook --target-dir target/hook
cd codenotch
npx @tauri-apps/cli@2 build --config tauri.bundle.conf.json
# → ..\target\release\bundle\nsis\Codenotch_<version>_x64-setup.exe
```

Tray menu: **Settings…**, **Refresh usage now**, **Quit**. Everything else is in the settings
window: the taskbar icon, which rings the notch shows, its size, start with Windows, the
language, Claude Code hooks, reset position, and the data folder (`%APPDATA%\codenotch` —
logs, persisted readings, icon overrides).

### Icons

Provider marks are the SVGs from [`@lobehub/icons-static-svg`](https://github.com/lobehub/lobe-icons)
(MIT), embedded unmodified — see `codenotch/glyphs/NOTICE.md`. Drop your own
`claude|codex|cursor|gemini.svg` (or `.png`) into `%APPDATA%\codenotch\glyphs\` to override.
The marks remain the trademarks of their owners.

## Layout

```
.
├── codenotch/          the Windows app (pill, hover card, settings, providers)
└── codenotch-hook/     tiny helper Claude Code calls to report session events
```

A pull request that touches this tree is built and tested; the check is skipped
inside forks until the pull request is opened here.

## Relationship to upstream

This port follows the upstream design and provider semantics. It is developed at
[Im-Midi/codenotch-windows](https://github.com/Im-Midi/codenotch-windows) and offered to the
upstream project as its `windows/` tree; the two are kept in sync. Session detection
originated in [Im-Midi/Pac-Man](https://github.com/Im-Midi/Pac-Man) (MIT).

## License

MIT — see `LICENSE`. The Codenotch design and name belong to the upstream author.
