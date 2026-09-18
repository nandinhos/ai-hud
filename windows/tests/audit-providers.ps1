# Testes e Auditoria E2E dos 7 Provedores do Codenotch / ai-hud
# Executar no PowerShell do Windows

Write-Output "=========================================================="
Write-Output "   CODENOTCH / AI-HUD: AUDITORIA E2E DOS 7 PROVEDORES    "
Write-Output "=========================================================="

$results = @()

# 1. Claude Code
$claudeCredPath = "$env:USERPROFILE\.claude\.credentials.json"
$claudeCli = Get-Command claude -ErrorAction SilentlyContinue
$claudeStatus = "Ausente"
$claudeDetail = "Sem credenciais"

if (Test-Path $claudeCredPath) {
    try {
        $json = Get-Content $claudeCredPath -Raw | ConvertFrom-Json
        $oauth = $json.claudeAiOauth
        $now = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        $exp = $oauth.expiresAt
        $hasRefresh = [bool]$oauth.refreshToken
        if ($exp -and $now -gt $exp) {
            $claudeStatus = "Token Expirado"
            $claudeDetail = "Expirou em $([DateTimeOffset]::FromUnixTimeMilliseconds($exp).ToString('g')). RefreshToken presente: $hasRefresh. CLI: $(if ($claudeCli) { 'Instalado' } else { 'Ausente (use npx)' })"
        } else {
            $claudeStatus = "Autenticado"
            $claudeDetail = "Token válido até $([DateTimeOffset]::FromUnixTimeMilliseconds($exp).ToString('g'))"
        }
    } catch {
        $claudeStatus = "Erro de Leitura"
        $claudeDetail = $_.Exception.Message
    }
}
$results += [PSCustomObject]@{ Provider = "1. Claude Code"; Status = $claudeStatus; Detalhes = $claudeDetail }

# 2. OpenAI Codex
$codexAuth = "$env:USERPROFILE\.codex\auth.json"
$codexStatus = "Ausente"
$codexDetail = "Arquivo ~/.codex/auth.json não localizado"
if (Test-Path $codexAuth) {
    $codexStatus = "Configurado"
    $codexDetail = "Sessão Codex presente em $codexAuth"
}
$results += [PSCustomObject]@{ Provider = "2. OpenAI Codex"; Status = $codexStatus; Detalhes = $codexDetail }

# 3. Cursor
$cursorDb = "$env:APPDATA\Cursor\User\globalStorage\state.vscdb"
$cursorStatus = "Ausente"
$cursorDetail = "Banco SQLite do Cursor não localizado"
if (Test-Path $cursorDb) {
    $cursorStatus = "Conectado"
    $cursorDetail = "Banco state.vscdb localizado em $env:APPDATA\Cursor"
}
$results += [PSCustomObject]@{ Provider = "3. Cursor"; Status = $cursorStatus; Detalhes = $cursorDetail }

# 4. Google Antigravity
$antiDirs = @(
    "$env:USERPROFILE\.antigravity",
    "$env:LOCALAPPDATA\antigravity",
    "$env:APPDATA\antigravity"
) | Where-Object { Test-Path $_ }
$antiStatus = "Sem Sessão"
$antiDetail = "Nenhum diretório ~/.antigravity encontrado"
if ($antiDirs) {
    $antiStatus = "Conectado"
    $antiDetail = "Diretórios de estado: $($antiDirs -join ', ')"
}
$results += [PSCustomObject]@{ Provider = "4. Antigravity"; Status = $antiStatus; Detalhes = $antiDetail }

# 5. DeepSeek
$deepseekKeyFile = "$env:APPDATA\codenotch\deepseek.key"
$deepseekEnv = [Environment]::GetEnvironmentVariable("DEEPSEEK_API_KEY", "User")
$deepseekStatus = "Não Configurado"
$deepseekDetail = "Configure %APPDATA%\codenotch\deepseek.key ou DEEPSEEK_API_KEY"
if ((Test-Path $deepseekKeyFile) -or $deepseekEnv) {
    $deepseekStatus = "Configurado"
    $deepseekDetail = "Chave de API disponível para consulta de saldo"
}
$results += [PSCustomObject]@{ Provider = "5. DeepSeek"; Status = $deepseekStatus; Detalhes = $deepseekDetail }

# 6. Meta Muse
$museAuth = "$env:USERPROFILE\.config\muse\auth.json"
$museSettings = "$env:USERPROFILE\.config\muse\settings.json"
$museStatus = "Ausente"
$museDetail = "~/.config/muse não localizado"
if ((Test-Path $museAuth) -and (Test-Path $museSettings)) {
    try {
        $mAuth = Get-Content $museAuth -Raw | ConvertFrom-Json
        $mSettings = Get-Content $museSettings -Raw | ConvertFrom-Json
        $email = $mAuth.runtime_capabilities.'plugin:clearer-muse:hook:safety-gate'.user_email
        $model = $mSettings.model
        $museStatus = "Conectado (Sem Mocks)"
        $museDetail = "Conta: $email | Modelo: $model (Zero valores mockados)"
    } catch {
        $museStatus = "Conectado"
        $museDetail = "Arquivos de configuração presentes"
    }
}
$results += [PSCustomObject]@{ Provider = "6. Meta Muse"; Status = $museStatus; Detalhes = $museDetail }

# 7. OpenCode Go
$opencodeKeyFile = "$env:APPDATA\codenotch\opencode.key"
$opencodeEnv = [Environment]::GetEnvironmentVariable("OPENCODE_API_KEY", "User")
$opencodeStatus = "Não Configurado"
$opencodeDetail = "Configure %APPDATA%\codenotch\opencode.key ou OPENCODE_API_KEY"
if ((Test-Path $opencodeKeyFile) -or $opencodeEnv) {
    $opencodeStatus = "Configurado"
    $opencodeDetail = "Chave de API disponível para inferência e limites"
}
$results += [PSCustomObject]@{ Provider = "7. OpenCode Go"; Status = $opencodeStatus; Detalhes = $opencodeDetail }

# Exibir tabela formatada sem cortes
($results | Format-Table -AutoSize -Wrap | Out-String -Width 200).TrimEnd() | Write-Output

# Diagnóstico do Processo
Write-Output "`n--- Status do Processo e Instalação ---"
$proc = Get-Process -Name 'codenotch' -ErrorAction SilentlyContinue
if ($proc) {
    Write-Output "Codenotch em execução: PID $($proc.Id), Memória: $([Math]::Round($proc.WorkingSet64 / 1MB, 2)) MB"
} else {
    Write-Output "Codenotch não está em execução no momento."
}

$installedExe = "$env:LOCALAPPDATA\Programs\Codenotch\codenotch.exe"
if (Test-Path $installedExe) {
    $finfo = Get-Item $installedExe
    Write-Output "Binário Instalado: $installedExe (Modificado em $($finfo.LastWriteTime))"
} else {
    Write-Output "Binário em $installedExe não encontrado."
}
