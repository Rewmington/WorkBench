# Captures the Anvil window so the PLAN 8.6 / 8.8 items that can only be proven with
# pixels actually get proven.
#
# ASCII-only on purpose: Windows PowerShell 5.1 decodes .ps1 as the ANSI codepage
# (GBK on zh-CN), and non-ASCII bytes inside the C# here-string silently eat line
# breaks -- which is exactly how this script first failed.
#
# Two modes:
#   Print (default) -- PrintWindow with PW_RENDERFULLCONTENT. Grabs the window's own
#       pixels even when it is behind another app, and does NOT steal focus. Preferred,
#       because raising the window interrupts whatever the human is doing.
#   Screen -- BitBlt the screen region. Only needed if PrintWindow comes back black
#       (a known WebView2 failure mode), and it requires the window to actually be on top.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools/capture.ps1 -Out shots/m0.png
param(
  [string]$Out = 'shots/anvil.png',
  [string]$ProcName = 'anvil',
  [ValidateSet('Print', 'Screen')]
  [string]$Mode = 'Print',
  [int]$Width = 0,
  [int]$Height = 0,
  [switch]$Maximize,
  [switch]$Restore,
  [switch]$Raise
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$code = @'
using System;
using System.Runtime.InteropServices;
public static class Win {
  [StructLayout(LayoutKind.Sequential)]
  public struct Rect { public int Left, Top, Right, Bottom; }

  // Without this, GetWindowRect hands back logical coordinates on a 125%/150% display
  // and the capture comes out offset and clipped.
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out Rect r);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out Rect r);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
}
'@
Add-Type -TypeDefinition $code
[void][Win]::SetProcessDPIAware()

$proc = Get-Process -Name $ProcName -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $proc) {
  Write-Error "no visible main window for process '$ProcName' (it may be hidden in the tray)"
  exit 2
}
$hwnd = $proc.MainWindowHandle

if ([Win]::IsIconic($hwnd)) { [void][Win]::ShowWindow($hwnd, 9) }   # SW_RESTORE
if ($Maximize) { [void][Win]::ShowWindow($hwnd, 3) }                # SW_MAXIMIZE
if ($Restore) { [void][Win]::ShowWindow($hwnd, 9) }
if ($Width -gt 0 -and $Height -gt 0) {
  # SIZEBOX | NOZORDER | NOACTIVATE = 0x40 | 0x04 | 0x10
  [void][Win]::SetWindowPos($hwnd, [IntPtr]::Zero, 0, 0, $Width, $Height, 0x54)
}
if ($Raise) { [void][Win]::SetForegroundWindow($hwnd) }
Start-Sleep -Milliseconds 400   # one composited frame, otherwise we capture the previous state

$rect = New-Object Win+Rect
[void][Win]::GetWindowRect($hwnd, [ref]$rect)
if ($Mode -eq 'Print') {
  # PW_CLIENTONLY paints the client area at (0,0), so size the bitmap to the client --
  # otherwise every shot carries a ~22px dead strip from the invisible DWM border.
  $client = New-Object Win+Rect
  [void][Win]::GetClientRect($hwnd, [ref]$client)
  $w = $client.Right
  $h = $client.Bottom
} else {
  $w = $rect.Right - $rect.Left
  $h = $rect.Bottom - $rect.Top
}
if ($w -le 0 -or $h -le 0) { Write-Error 'window rect came back empty'; exit 3 }

$bmp = New-Object System.Drawing.Bitmap($w, $h, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$gfx = [System.Drawing.Graphics]::FromImage($bmp)

if ($Mode -eq 'Print') {
  $hdc = $gfx.GetHdc()
  # PW_CLIENTONLY(1) | PW_RENDERFULLCONTENT(2): the flag that makes WebView2 paint
  [void][Win]::PrintWindow($hwnd, $hdc, 3)
  $gfx.ReleaseHdc($hdc)
} else {
  $gfx.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size($w, $h)))
}

# A black frame is PrintWindow's known failure mode on some GPU/composition setups.
# Say so instead of handing back a dark rectangle that looks like "the app is dark themed".
$samples = 0
$lit = 0
for ($y = 0; $y -lt $h; $y += 37) {
  for ($x = 0; $x -lt $w; $x += 31) {
    $p = $bmp.GetPixel($x, $y)
    $samples++
    if ($p.R -gt 24 -or $p.G -gt 24 -or $p.B -gt 24) { $lit++ }
  }
}
$ratio = if ($samples) { $lit / $samples } else { 0 }

$dir = Split-Path -Parent $Out
if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
$abs = Join-Path (Get-Location) $Out
$bmp.Save($abs, [System.Drawing.Imaging.ImageFormat]::Png)
$gfx.Dispose(); $bmp.Dispose()

Write-Output ("saved {0}  {1}x{2}  mode={3}  visible={4}  lit={5:P1}" -f $abs, $w, $h, $Mode, [Win]::IsWindowVisible($hwnd), $ratio)
if ($ratio -lt 0.02) {
  Write-Output 'WARNING: capture is essentially black -- PrintWindow could not read the webview, retry with -Mode Screen -Raise'
}
