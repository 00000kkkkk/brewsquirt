$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$Repo = "00000kkkkk/javasquirt"
$InstallDir = if ($env:JSQ_INSTALL_DIR) { $env:JSQ_INSTALL_DIR } else { Join-Path $env:USERPROFILE ".jsq\bin" }
$Version = $env:JSQ_VERSION

$Arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
if ($Arch -ne "AMD64" -and $Arch -ne "ARM64") {
  throw "install.ps1: unsupported CPU architecture $Arch"
}
if ($Arch -eq "ARM64") {
  Write-Host "no native ARM64 build yet, installing the x64 build (runs under emulation)"
}

if (-not $Version) {
  $Version = (Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest" -Headers @{ "User-Agent" = "jsq-install" }).tag_name
}
if (-not $Version.StartsWith("v")) {
  $Version = "v$Version"
}

$Name = "javasquirt-$Version-x86_64-pc-windows-msvc"
$Base = "https://github.com/$Repo/releases/download/$Version"
$Tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("jsq-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $Tmp | Out-Null

try {
  Write-Host "downloading JavaSquirt $Version for x86_64-pc-windows-msvc"
  Invoke-WebRequest "$Base/$Name.zip" -OutFile (Join-Path $Tmp "$Name.zip") -UseBasicParsing
  Invoke-WebRequest "$Base/$Name.sha256" -OutFile (Join-Path $Tmp "$Name.sha256") -UseBasicParsing

  $Expected = ((Get-Content (Join-Path $Tmp "$Name.sha256") -Raw).Trim() -split "\s+")[0]
  $Actual = (Get-FileHash (Join-Path $Tmp "$Name.zip") -Algorithm SHA256).Hash
  if ($Actual -ne $Expected) {
    throw "install.ps1: checksum mismatch for $Name.zip"
  }

  $Extract = Join-Path $Tmp "x"
  Expand-Archive -Path (Join-Path $Tmp "$Name.zip") -DestinationPath $Extract -Force
  New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
  Copy-Item (Join-Path $Extract "jsq.exe"), (Join-Path $Extract "vrotdaem.exe") -Destination $InstallDir -Force
} finally {
  Remove-Item $Tmp -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "installed jsq and vrotdaem $Version to $InstallDir"

if (-not $env:JSQ_NO_MODIFY_PATH) {
  $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
  $Parts = @()
  if ($UserPath) { $Parts = $UserPath -split ";" }
  if ($Parts -notcontains $InstallDir) {
    [Environment]::SetEnvironmentVariable("Path", "$InstallDir;$UserPath", "User")
    Write-Host "added $InstallDir to your user PATH"
  }
  if (($env:Path -split ";") -notcontains $InstallDir) {
    $env:Path = "$InstallDir;$env:Path"
  }
  Write-Host "open a new terminal, then run 'jsq' to start the REPL or 'jsq app.jsq' to run a file"
} else {
  Write-Host "add $InstallDir to your PATH to run jsq from anywhere"
}
