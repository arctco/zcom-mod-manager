$ErrorActionPreference = "Stop"
$Version = "0.1.5"
$Archive = "retoc_cli-x86_64-pc-windows-msvc.zip"
$Base = "https://github.com/trumank/retoc/releases/download/v$Version"
$Temporary = Join-Path $env:TEMP ("zcom-retoc-" + [guid]::NewGuid())
$Output = "src-tauri/binaries/retoc-x86_64-pc-windows-msvc.exe"
New-Item -ItemType Directory -Force $Temporary, "src-tauri/binaries" | Out-Null
try {
  Invoke-WebRequest "$Base/$Archive" -OutFile (Join-Path $Temporary $Archive)
  Invoke-WebRequest "$Base/$Archive.sha256" -OutFile (Join-Path $Temporary "$Archive.sha256")
  $Expected = ((Get-Content (Join-Path $Temporary "$Archive.sha256")) -split '\s+')[0].ToLowerInvariant()
  $Actual = (Get-FileHash (Join-Path $Temporary $Archive) -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($Expected -ne $Actual) { throw "retoc checksum mismatch" }
  Expand-Archive (Join-Path $Temporary $Archive) -DestinationPath $Temporary
  $Binary = Get-ChildItem $Temporary -Recurse -Filter "retoc.exe" | Select-Object -First 1
  if (-not $Binary) { throw "retoc.exe was not present in the verified archive" }
  Copy-Item $Binary.FullName $Output -Force
  & $Output --version
} finally {
  Remove-Item $Temporary -Recurse -Force -ErrorAction SilentlyContinue
}
