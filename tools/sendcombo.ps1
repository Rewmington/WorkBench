# Sends one system-wide key combination, so a GLOBAL hotkey can be proven the way a
# human would press it: keybd_event goes through the OS input queue, which is the only
# path that reaches RegisterHotKey. Dispatching a KeyboardEvent inside the webview proves
# nothing about the native binding.
#
# ASCII-only on purpose: Windows PowerShell 5.1 decodes .ps1 as the ANSI codepage
# (GBK on zh-CN), so non-ASCII bytes silently eat line breaks.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools/sendcombo.ps1 -Keys 'ALT,SHIFT,A'
param(
  [string]$Keys = 'ALT,SHIFT,A'
)

$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class Kbd {
  [DllImport("user32.dll")]
  public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
}
'@

$KEYUP = 0x2

function Vk([string]$k) {
  switch ($k) {
    'ALT' { return 0x12 }
    'CTRL' { return 0x11 }
    'SHIFT' { return 0x10 }
    'LWIN' { return 0x5B }
    'ESC' { return 0x1B }
    'SPACE' { return 0x20 }
    'ENTER' { return 0x0D }
  }
  if ($k.Length -eq 1) {
    # Letters map to VK_A..VK_Z and digits to VK_0..VK_9 by their own char codes.
    return [byte][char]$k.ToUpper()
  }
  throw "unknown key name: $k"
}

$seq = @($Keys.Split(',') | ForEach-Object { $_.Trim() } | Where-Object { $_ } | ForEach-Object { Vk $_ })
if ($seq.Count -eq 0) { throw 'nothing to send' }

foreach ($vk in $seq) {
  [Kbd]::keybd_event([byte]$vk, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 30
}
[array]::Reverse($seq)
foreach ($vk in $seq) {
  [Kbd]::keybd_event([byte]$vk, 0, $KEYUP, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 30
}

Write-Output ("sent: " + ($seq -join '+'))
