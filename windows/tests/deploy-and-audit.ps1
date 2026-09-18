Stop-Process -Name codenotch -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 1

$src = "C:\Users\atiga\projects\codenotch\windows\target\release\codenotch.exe"
$dst = "$env:LOCALAPPDATA\Programs\Codenotch\codenotch.exe"

if (Test-Path $src) {
    Copy-Item -Path $src -Destination $dst -Force
    Write-Output "Binario copiado com sucesso para $dst"
} else {
    Write-Error "Arquivo fonte $src nao encontrado"
}

Start-Process $dst
Start-Sleep -Seconds 2

& "C:\Users\atiga\projects\codenotch\windows\tests\audit-providers.ps1"
