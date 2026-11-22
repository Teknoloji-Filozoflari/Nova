param(
    [Parameter(Mandatory = $true)][string]$Tag,
    [Parameter(Mandatory = $true)][string]$Binary,
    [Parameter(Mandatory = $true)][string]$OutDir,
    [string]$Target = "x86_64-pc-windows-msvc"
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$StageName = "nova-$Tag-$Target"
$Stage = Join-Path $OutDir $StageName
New-Item -ItemType Directory -Force -Path $Stage | Out-Null
Copy-Item $Binary (Join-Path $Stage "nova.exe")
Copy-Item (Join-Path $Root "LICENSE.md"), `
    (Join-Path $Root "LICENSE-MIT"), `
    (Join-Path $Root "LICENSE-APACHE"), `
    (Join-Path $Root "README.md") -Destination $Stage
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$Zip = Join-Path $OutDir "$StageName.zip"
if (Test-Path $Zip) { Remove-Item $Zip }
Compress-Archive -Path $Stage -DestinationPath $Zip
Write-Output $Zip
