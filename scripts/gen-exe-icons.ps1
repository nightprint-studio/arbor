# gen-exe-icons.ps1 - regenerate the .ico every headless backend embeds in its executable.
#
# Source of truth is src-tauri/icons/products/<product>.png, the same artwork the launcher
# draws; this only re-encodes it. Output is <be-crate>/icon.ico, beside the Cargo.toml that
# names it, so a crate carries its own icon and can be moved without losing it.
#
# Each .ico holds 16/32/48 as 32-bit BGRA bitmaps and 256 as an embedded PNG - the sizes
# Explorer, the taskbar and the Alt-Tab switcher actually ask for. A single 256 entry would
# leave Windows to downscale to 16px on its own, which it does badly.
#
# Windows-only, System.Drawing-only, no packages: it produces a committed artefact, so it runs
# on the machine that changes the artwork and never during a build.
#
# Usage:  powershell -ExecutionPolicy Bypass -File scripts/gen-exe-icons.ps1

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$root    = Split-Path -Parent $PSScriptRoot
$sources = Join-Path $root 'src-tauri\icons\products'

# product -> the crate whose [[bin]] gets the icon. Not derived from a glob: a product without
# a backend has no executable to decorate, and a silent mismatch here is an icon that quietly
# stops being regenerated.
$targets = [ordered]@{
    'bennu'    = 'crates\products\bennu\runtime\be'
    'corvus'   = 'crates\products\corvus\be'
    'garrulus' = 'crates\products\garrulus\be'
    'merula'   = 'crates\products\merula\be'
    'picus'    = 'crates\products\picus\be'
    'sitta'    = 'crates\products\sitta\be'
    'tyto'     = 'crates\products\tyto\be'
}

$BITMAP_SIZES = @(16, 32, 48)
$PNG_SIZE     = 256

# Scale to a square of `size`, keeping the alpha channel intact.
#
# SourceCopy rather than the default SourceOver: the destination starts fully transparent, and
# blending onto transparency darkens every edge pixel of an antialiased logo.
function New-Scaled {
    param([System.Drawing.Image]$Image, [int]$Size)

    $bmp = New-Object System.Drawing.Bitmap $Size, $Size, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g   = [System.Drawing.Graphics]::FromImage($bmp)
    try {
        $g.CompositingMode   = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
        $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $g.PixelOffsetMode   = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
        $g.SmoothingMode     = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
        $g.DrawImage($Image, (New-Object System.Drawing.Rectangle 0, 0, $Size, $Size))
    } finally {
        $g.Dispose()
    }
    return $bmp
}

# One icon entry in the classic shape: BITMAPINFOHEADER, the pixels bottom-up, then the AND
# mask. The header claims twice the real height because the mask counts as a second image -
# an ICO quirk, not a mistake. The mask itself is all zeros: the alpha channel already says
# what is transparent, and Windows honours it for 32-bit entries.
function Get-DibBytes {
    param([System.Drawing.Bitmap]$Bitmap)

    $w = $Bitmap.Width
    $h = $Bitmap.Height
    $rect = New-Object System.Drawing.Rectangle 0, 0, $w, $h
    $data = $Bitmap.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadOnly,
                             [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    try {
        $stride = $data.Stride
        $px = New-Object byte[] ($stride * $h)
        [System.Runtime.InteropServices.Marshal]::Copy($data.Scan0, $px, 0, $px.Length)
    } finally {
        $Bitmap.UnlockBits($data)
    }

    $out = New-Object System.Collections.Generic.List[byte]
    $out.AddRange([BitConverter]::GetBytes([uint32]40))      # biSize
    $out.AddRange([BitConverter]::GetBytes([int32]$w))       # biWidth
    $out.AddRange([BitConverter]::GetBytes([int32]($h * 2))) # biHeight (pixels + mask)
    $out.AddRange([BitConverter]::GetBytes([uint16]1))       # biPlanes
    $out.AddRange([BitConverter]::GetBytes([uint16]32))      # biBitCount
    $out.AddRange([BitConverter]::GetBytes([uint32]0))       # biCompression = BI_RGB
    $out.AddRange([BitConverter]::GetBytes([uint32]($w * $h * 4)))  # biSizeImage
    $out.AddRange([BitConverter]::GetBytes([int32]0))        # biXPelsPerMeter
    $out.AddRange([BitConverter]::GetBytes([int32]0))        # biYPelsPerMeter
    $out.AddRange([BitConverter]::GetBytes([uint32]0))       # biClrUsed
    $out.AddRange([BitConverter]::GetBytes([uint32]0))       # biClrImportant

    for ($y = $h - 1; $y -ge 0; $y--) {
        $row = New-Object byte[] ($w * 4)
        [Array]::Copy($px, $y * $stride, $row, 0, $w * 4)
        $out.AddRange($row)
    }

    $maskStride = [int]([math]::Floor(($w + 31) / 32)) * 4
    $out.AddRange((New-Object byte[] ($maskStride * $h)))

    return ,$out.ToArray()
}

function Get-PngBytes {
    param([System.Drawing.Bitmap]$Bitmap)

    $ms = New-Object System.IO.MemoryStream
    try {
        $Bitmap.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
        return ,$ms.ToArray()
    } finally {
        $ms.Dispose()
    }
}

function Write-Ico {
    param([string]$SourcePng, [string]$Destination)

    $image   = [System.Drawing.Image]::FromFile($SourcePng)
    $entries = New-Object System.Collections.Generic.List[object]
    try {
        foreach ($size in $BITMAP_SIZES) {
            $scaled = New-Scaled -Image $image -Size $size
            try {
                $entries.Add([pscustomobject]@{ Size = $size; Bytes = (Get-DibBytes -Bitmap $scaled) })
            } finally {
                $scaled.Dispose()
            }
        }
        $scaled = New-Scaled -Image $image -Size $PNG_SIZE
        try {
            $entries.Add([pscustomobject]@{ Size = $PNG_SIZE; Bytes = (Get-PngBytes -Bitmap $scaled) })
        } finally {
            $scaled.Dispose()
        }
    } finally {
        $image.Dispose()
    }

    $ico = New-Object System.Collections.Generic.List[byte]
    $ico.AddRange([BitConverter]::GetBytes([uint16]0))              # reserved
    $ico.AddRange([BitConverter]::GetBytes([uint16]1))              # type = icon
    $ico.AddRange([BitConverter]::GetBytes([uint16]$entries.Count))

    # 6-byte directory header, then one 16-byte entry each, then the images back to back.
    $offset = 6 + (16 * $entries.Count)
    foreach ($e in $entries) {
        # 256 does not fit in a byte and is written as 0 - the format's way of saying "the
        # large one", which is why a 256 entry looks like a zero-sized one to a naive reader.
        $dim = if ($e.Size -ge 256) { 0 } else { $e.Size }
        $ico.Add([byte]$dim)                                   # width
        $ico.Add([byte]$dim)                                   # height
        $ico.Add([byte]0)                                      # palette entries
        $ico.Add([byte]0)                                      # reserved
        $ico.AddRange([BitConverter]::GetBytes([uint16]1))     # colour planes
        $ico.AddRange([BitConverter]::GetBytes([uint16]32))    # bits per pixel
        $ico.AddRange([BitConverter]::GetBytes([uint32]$e.Bytes.Length))
        $ico.AddRange([BitConverter]::GetBytes([uint32]$offset))
        $offset += $e.Bytes.Length
    }
    foreach ($e in $entries) { $ico.AddRange([byte[]]$e.Bytes) }

    [System.IO.File]::WriteAllBytes($Destination, $ico.ToArray())
}

$written = 0
foreach ($product in $targets.Keys) {
    $png = Join-Path $sources "$product.png"
    if (-not (Test-Path $png)) { throw "no artwork for '$product' at $png" }

    $crate = Join-Path $root $targets[$product]
    if (-not (Test-Path (Join-Path $crate 'Cargo.toml'))) { throw "no crate at $crate" }

    $ico = Join-Path $crate 'icon.ico'
    Write-Ico -SourcePng $png -Destination $ico
    Write-Host ("{0,-10} -> {1}" -f $product, $ico)
    $written++
}
Write-Host "$written icons written."
exit 0
