<#
.SYNOPSIS
  Serves one launcher update on http://127.0.0.1:<Port>/ for the slice 5 gate (#89).

.DESCRIPTION
  Gate builds only (tag launcher-v<version>-gate): their updater endpoint is
  http://127.0.0.1:8789/latest.json (src-tauri/gate/tauri.gate.conf.json). Release
  builds never read this endpoint. The script writes latest.json in memory from
  -Installer, -Signature and -Version, and serves it and the installer bytes until
  you press Ctrl+C. It listens on the loopback address only and needs no elevation.
  Nothing on disk is changed: -Tamper flips one byte of the copy it serves.

  Scenarios (docs/release/launcher-signing.md, gate builds):
    good       -Installer N+1-setup.exe -Signature N+1-setup.exe.sig
    tampered   the same plus -Tamper
    wrong key  -Signature wrong-key.sig
    unsigned   -Unsigned (latest.json carries an empty signature)
    restored   -Signature made with the key restored from the recovery copy

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File serve-update.ps1 -Installer .\n1\Project-Resurrected_0.1.1_x64-setup.exe -Signature .\n1\Project-Resurrected_0.1.1_x64-setup.exe.sig -Version 0.1.1
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$Installer,
  [string]$Signature,
  [Parameter(Mandatory = $true)][string]$Version,
  [switch]$Unsigned,
  [switch]$Tamper,
  [int]$Port = 8789
)
$ErrorActionPreference = 'Stop'

if ($Unsigned -eq [bool]$Signature) { throw 'Pass exactly one of -Signature or -Unsigned.' }
$Installer = (Resolve-Path -LiteralPath $Installer).Path
$name = [IO.Path]::GetFileName($Installer)
# A spaced name (an unrenamed Tauri bundle) must still match the client's %20 request.
$urlName = [Uri]::EscapeDataString($name)
$bytes = [IO.File]::ReadAllBytes($Installer)
if ($Tamper) { $bytes[[int]($bytes.Length / 2)] = $bytes[[int]($bytes.Length / 2)] -bxor 0xff }
$sig = ''
if (-not $Unsigned) { $sig = ([IO.File]::ReadAllText((Resolve-Path -LiteralPath $Signature).Path)).Trim() }

$listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, $Port)
$listener.Start()
$Port = ([Net.IPEndPoint]$listener.LocalEndpoint).Port
$latest = [ordered]@{
  version   = $Version
  notes     = "Launcher $Version (slice 5 gate, local)"
  pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
  platforms = [ordered]@{
    'windows-x86_64' = [ordered]@{ signature = $sig; url = "http://127.0.0.1:$Port/$urlName" }
  }
}
$json = [Text.Encoding]::UTF8.GetBytes(($latest | ConvertTo-Json -Depth 5))
$routes = @{ '/latest.json' = @('application/json', $json); "/$urlName" = @('application/octet-stream', $bytes) }

# Tests read this line to learn the port.
[Console]::Out.WriteLine("Serving http://127.0.0.1:$Port/latest.json (version $Version, $name, tamper=$Tamper, unsigned=$Unsigned)")
[Console]::Out.Flush()

try {
  while ($true) {
    $client = $listener.AcceptTcpClient()
    try {
      $stream = $client.GetStream()
      $reader = [IO.StreamReader]::new($stream, [Text.Encoding]::ASCII, $false, 4096, $true)
      $request = $reader.ReadLine()
      while ($true) { $line = $reader.ReadLine(); if ($null -eq $line -or $line -eq '') { break } }
      $path = if ($request -match '^(GET|HEAD) (\S+) HTTP/1\.[01]$') { $Matches[2].Split('?')[0] } else { '' }
      $route = $routes[$path]
      if ($route) { $status = '200 OK'; $type = $route[0]; $body = [byte[]]$route[1] }
      else { $status = '404 Not Found'; $type = 'text/plain'; $body = [Text.Encoding]::ASCII.GetBytes('not found') }
      $head = "HTTP/1.1 $status`r`nContent-Type: $type`r`nContent-Length: $($body.Length)`r`nConnection: close`r`n`r`n"
      $h = [Text.Encoding]::ASCII.GetBytes($head)
      $stream.Write($h, 0, $h.Length)
      if ($request -notmatch '^HEAD ') { $stream.Write($body, 0, $body.Length) }
      $stream.Flush()
      [Console]::Out.WriteLine("$(Get-Date -Format HH:mm:ss) $request -> $status")
      [Console]::Out.Flush()
    } catch {
      [Console]::Out.WriteLine("$(Get-Date -Format HH:mm:ss) connection error: $($_.Exception.Message)")
    } finally {
      $client.Close()
    }
  }
} finally {
  $listener.Stop()
}
