<#
  ModManager BeamNG: сбор сведений о BeamNG.drive на этом компьютере.

  Скрипт только ЧИТАЕТ: ничего не меняет, не удаляет, не устанавливает и никуда не отправляет.
  Результат: архив bmm-<время>-<метка>.zip на рабочем столе. Его и нужно прислать.

  Запуск (PowerShell, из папки со скриптом):
    powershell -ExecutionPolicy Bypass -File .\collect-beamng-info.ps1 -Label B0-kak-est

  Если папка пользователя игры перенесена в своё место:
    ... -UserFolder "D:\Games\BeamNG-user"
#>
param(
    [string]$Label = "snapshot",
    [string]$UserFolder = "",
    [string]$GameDir = "",
    [string]$OutDir = "",
    [string]$LocalAppData = $env:LOCALAPPDATA,
    [int]$MaxZips = 3000,
    [int]$MaxListEntries = 30000,
    [switch]$LockTest   # проверять занятость архивов, даже если процесс игры не найден
)

Set-StrictMode -Off
$ErrorActionPreference = 'Continue'
try { Add-Type -AssemblyName System.IO.Compression } catch {}
try { Add-Type -AssemblyName System.IO.Compression.FileSystem } catch {}

$Utf8Bom = New-Object System.Text.UTF8Encoding($true)
$safeLabel = ($Label -replace '[^\w\-]', '_')
if (-not $OutDir) { $OutDir = [Environment]::GetFolderPath('Desktop') }
if (-not $OutDir) { $OutDir = (Get-Location).Path }
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$work = Join-Path $OutDir "bmm-$stamp-$safeLabel"
New-Item -ItemType Directory -Force -Path $work | Out-Null

$summary = New-Object System.Collections.Generic.List[string]
function Say([string]$s) { $summary.Add($s); Write-Host $s }
function P { param([Parameter(ValueFromRemainingArguments = $true)][string[]]$parts) [System.IO.Path]::Combine([string[]]$parts) }
function SaveText([string]$name, $text) { [System.IO.File]::WriteAllText((P $work $name), [string]$text, $Utf8Bom) }
function SaveJson([string]$name, $obj) { SaveText $name ($obj | ConvertTo-Json -Depth 8) }
function Reg([string]$path, [string]$name) { try { (Get-ItemProperty -LiteralPath $path -Name $name -ErrorAction Stop).$name } catch { $null } }
function Rel([string]$root, [string]$full) { $full.Substring([Math]::Min($root.Length, $full.Length)).TrimStart('\', '/') }

# Копирует небольшой файл как есть. Большой лог обрезается до хвоста (там самое свежее).
function CopySmall([string]$src, [string]$dstName, [long]$max = 20MB) {
    try {
        if (-not [System.IO.File]::Exists($src)) { return $false }
        $fi = New-Object System.IO.FileInfo($src)
        $dst = P $work $dstName
        if ($fi.Length -le $max) {
            # Открываем с разрешением чужой записи: игра может держать лог открытым.
            $in = [System.IO.File]::Open($src, 'Open', 'Read', 'ReadWrite')
            try { $out = [System.IO.File]::Create($dst); try { $in.CopyTo($out) } finally { $out.Close() } } finally { $in.Close() }
        } else {
            $in = [System.IO.File]::Open($src, 'Open', 'Read', 'ReadWrite')
            try {
                [void]$in.Seek(-$max, 'End')
                $out = [System.IO.File]::Create($dst + '.tail'); try { $in.CopyTo($out) } finally { $out.Close() }
            } finally { $in.Close() }
        }
        return $true
    } catch { Say "  не удалось скопировать ${src}: $($_.Exception.Message)"; return $false }
}

# Список файлов и папок (относительный путь, размер, время, атрибуты) в CSV.
function ListTree([string]$root, [int]$depth, [string]$csvName) {
    if (-not [System.IO.Directory]::Exists($root)) { return 0 }
    $rows = New-Object System.Collections.Generic.List[object]
    $stack = New-Object System.Collections.Generic.Stack[object]
    $stack.Push(@($root, 0))
    while ($stack.Count -gt 0 -and $rows.Count -lt $MaxListEntries) {
        $item = $stack.Pop(); $dir = [string]$item[0]; $d = [int]$item[1]
        try { $entries = (New-Object System.IO.DirectoryInfo($dir)).GetFileSystemInfos() } catch { $rows.Add([pscustomobject]@{ Path = (Rel $root $dir); Type = 'error'; Size = ''; LastWriteUtc = ''; Attributes = $_.Exception.Message }); continue }
        foreach ($e in $entries) {
            $isDir = ($e.Attributes -band [System.IO.FileAttributes]::Directory) -ne 0
            $rows.Add([pscustomobject]@{
                Path = (Rel $root $e.FullName); Type = $(if ($isDir) { 'dir' } else { 'file' })
                Size = $(if ($isDir) { '' } else { $e.Length }); LastWriteUtc = $e.LastWriteTimeUtc.ToString('o'); Attributes = [string]$e.Attributes
            })
            $isLink = ($e.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0
            if ($isDir -and -not $isLink -and $d + 1 -lt $depth) { $stack.Push(@($e.FullName, ($d + 1))) }
        }
    }
    $rows | Sort-Object Path | Export-Csv -LiteralPath (P $work $csvName) -NoTypeInformation -Encoding UTF8
    if ($rows.Count -ge $MaxListEntries) { Say "  список $csvName обрезан на $MaxListEntries записях" }
    return $rows.Count
}

$running = @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.ProcessName -like '*BeamNG*' })
$gameRunning = $running.Count -gt 0

Say "ModManager BeamNG: сбор сведений, метка «$Label», $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')"
Say "Игра запущена: $(if ($gameRunning) { 'да (' + (($running | ForEach-Object { $_.ProcessName }) -join ', ') + ')' } else { 'нет' })"

# ---------------------------------------------------------------- 1. Компьютер
Write-Host "1/5 Компьютер..."
$sys = [ordered]@{
    label = $Label; time = (Get-Date).ToString('o'); timeZone = [TimeZoneInfo]::Local.Id
    culture = (Get-Culture).Name; uiCulture = (Get-UICulture).Name; powershell = $PSVersionTable.PSVersion.ToString()
    os = [Environment]::OSVersion.VersionString; is64 = [Environment]::Is64BitOperatingSystem
}
try { $os = Get-CimInstance Win32_OperatingSystem -ErrorAction Stop; $sys.os = "$($os.Caption) $($os.Version) build $($os.BuildNumber)"; $sys.osArch = $os.OSArchitecture } catch {}
try { $sys.cpu = (Get-CimInstance Win32_Processor -ErrorAction Stop | Select-Object -First 1).Name } catch {}
try { $sys.ramGB = [math]::Round((Get-CimInstance Win32_ComputerSystem -ErrorAction Stop).TotalPhysicalMemory / 1GB, 1) } catch {}
try {
    $sys.gpu = @(Get-CimInstance Win32_VideoController -ErrorAction Stop | ForEach-Object {
        [ordered]@{ name = $_.Name; driver = $_.DriverVersion; mode = "$($_.CurrentHorizontalResolution)x$($_.CurrentVerticalResolution) @ $($_.CurrentRefreshRate) Hz" } })
} catch {}
try { $sys.monitors = @(Get-CimInstance -Namespace root\wmi -ClassName WmiMonitorBasicDisplayParams -ErrorAction Stop | ForEach-Object { "$($_.InstanceName) $($_.MaxHorizontalImageSize)x$($_.MaxVerticalImageSize) см" }) } catch {}
$sys.appliedDpi = Reg 'HKCU:\Control Panel\Desktop\WindowMetrics' 'AppliedDPI'
$sys.logPixels = Reg 'HKCU:\Control Panel\Desktop' 'LogPixels'
$sys.transparencyEffects = Reg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' 'EnableTransparency'
$sys.appsUseLightTheme = Reg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' 'AppsUseLightTheme'
$sys.longPathsEnabled = Reg 'HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem' 'LongPathsEnabled'
try {
    Add-Type -Namespace Bmm -Name Spi -MemberDefinition '[DllImport("user32.dll")] public static extern bool SystemParametersInfo(int a, int b, ref int c, int d);' -ErrorAction Stop
    $v = 0; [void][Bmm.Spi]::SystemParametersInfo(0x1042, 0, [ref]$v, 0); $sys.animationEffects = [bool]$v   # «Эффекты анимации»
} catch {}
$wv = Reg 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' 'pv'
if (-not $wv) { $wv = Reg 'HKCU:\Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' 'pv' }
$sys.webView2 = $wv
try { $sys.dotnetSdks = ((& dotnet --list-sdks 2>$null) -join '; ') } catch { $sys.dotnetSdks = $null }
try {
    $mp = Get-MpComputerStatus -ErrorAction Stop
    $sys.defenderRealtime = $mp.RealTimeProtectionEnabled
    $sys.defenderExclusions = @((Get-MpPreference -ErrorAction Stop).ExclusionPath)
} catch {}
$sys.beamngProcesses = @($running | ForEach-Object { $p = $_; $path = ''; try { $path = $p.Path } catch {}; "$($p.ProcessName) pid=$($p.Id) $path" })
SaveJson 'system.json' $sys

# ---------------------------------------------------------------- 2. Steam и папка игры
Write-Host "2/5 Steam и папка игры..."
$steam = Reg 'HKCU:\Software\Valve\Steam' 'SteamPath'
if (-not $steam) { $steam = Reg 'HKLM:\SOFTWARE\WOW6432Node\Valve\Steam' 'InstallPath' }
$steamInfo = [ordered]@{ steamPath = $steam; libraries = @(); appManifest = $null; gameDir = $null }
if ($steam) {
    $steam = $steam -replace '/', '\'
    $libsFile = P $steam 'steamapps' 'libraryfolders.vdf'
    [void](CopySmall $libsFile 'steam-libraryfolders.vdf' 1MB)
    $libs = @($steam)
    if ([System.IO.File]::Exists($libsFile)) {
        foreach ($m in [regex]::Matches([System.IO.File]::ReadAllText($libsFile), '"path"\s+"([^"]+)"')) { $libs += ($m.Groups[1].Value -replace '\\\\', '\') }
    }
    $steamInfo.libraries = @($libs | Select-Object -Unique)
    foreach ($lib in $steamInfo.libraries) {
        $acf = P $lib 'steamapps' 'appmanifest_284160.acf'   # 284160 — BeamNG.drive в Steam
        if ([System.IO.File]::Exists($acf)) {
            $steamInfo.appManifest = $acf
            [void](CopySmall $acf 'steam-appmanifest_284160.acf' 1MB)
            $m = [regex]::Match([System.IO.File]::ReadAllText($acf), '"installdir"\s+"([^"]+)"')
            if ($m.Success) { $gd = P $lib 'steamapps' 'common' $m.Groups[1].Value; if ([System.IO.Directory]::Exists($gd)) { $steamInfo.gameDir = $gd } }
        }
    }
}
if ($GameDir) { $steamInfo.gameDir = $GameDir }
SaveJson 'steam.json' $steamInfo
if ($steamInfo.gameDir) {
    Say "Папка игры: $($steamInfo.gameDir)"
    [void](ListTree $steamInfo.gameDir 2 'game-dir.csv')
    # Небольшие текстовые файлы в корне: среди них может быть версия игры и настройка папки пользователя (startup.ini).
    foreach ($f in [System.IO.Directory]::GetFiles($steamInfo.gameDir)) {
        $n = [System.IO.Path]::GetFileName($f)
        if ($n -match '\.(ini|txt|json|cfg)$' -and (New-Object System.IO.FileInfo($f)).Length -lt 256KB) { [void](CopySmall $f ("game-root-" + $n) 256KB) }
    }
} else { Say "Папка игры в Steam не найдена (можно указать -GameDir)" }

# ---------------------------------------------------------------- 3. Папки пользователя BeamNG
Write-Host "3/5 Папки пользователя..."
$candidates = New-Object System.Collections.Generic.List[string]
if ($UserFolder) { $candidates.Add($UserFolder) }
if ($LocalAppData) {
    $newRoot = P $LocalAppData 'BeamNG' 'BeamNG.drive'      # с 0.37: ...\BeamNG\BeamNG.drive\current
    $oldRoot = P $LocalAppData 'BeamNG.drive'               # до 0.37: ...\BeamNG.drive\<версия>
    foreach ($root in @((P $LocalAppData 'BeamNG'), $oldRoot)) { if ([System.IO.Directory]::Exists($root)) { [void](ListTree $root 3 ("local-" + [System.IO.Path]::GetFileName($root) + ".csv")) } }
    foreach ($root in @($newRoot, $oldRoot)) {
        if ([System.IO.Directory]::Exists($root)) {
            foreach ($d in [System.IO.Directory]::GetDirectories($root)) { if ([System.IO.Directory]::Exists((P $d 'mods'))) { $candidates.Add($d) } }
        }
    }
}
$userFolders = @($candidates | Select-Object -Unique)
Say "Найдено папок пользователя с mods: $($userFolders.Count)"
$folderReports = @()
$i = 0
foreach ($u in $userFolders) {
    $i++
    $tag = "user$i"
    $mods = P $u 'mods'
    Say "[$tag] $u"
    $rep = [ordered]@{ tag = $tag; path = $u; modsExists = [System.IO.Directory]::Exists($mods) }
    [void](ListTree $u 1 "$tag-root.csv")
    if ($rep.modsExists) {
        $rep.modsEntries = ListTree $mods 6 "$tag-mods.csv"
        $rep.dbJson = CopySmall (P $mods 'db.json') "$tag-db.json" 20MB
        foreach ($f in [System.IO.Directory]::GetFiles($mods)) {
            $n = [System.IO.Path]::GetFileName($f)
            if ($n -ne 'db.json' -and $n -match '\.(json|txt|ini)$') { [void](CopySmall $f "$tag-mods-$n" 5MB) }
        }
        Say "  mods: записей $($rep.modsEntries), db.json: $(if ($rep.dbJson) { 'скопирован' } else { 'нет' })"
    }
    foreach ($f in [System.IO.Directory]::GetFiles($u)) {
        $n = [System.IO.Path]::GetFileName($f)
        if ($n -match '\.(log|txt|ini|json)$') { [void](CopySmall $f "$tag-$n" 8MB) }
    }
    $folderReports += $rep
}
SaveJson 'user-folders.json' $folderReports

# ---------------------------------------------------------------- 4. Содержимое архивов модов
Write-Host "4/5 Архивы модов (может занять пару минут)..."
$zips = @()
foreach ($u in $userFolders) {
    $mods = P $u 'mods'
    if (-not [System.IO.Directory]::Exists($mods)) { continue }
    $files = @()
    try { $files = [System.IO.Directory]::GetFiles($mods, '*', [System.IO.SearchOption]::AllDirectories) } catch { Say "  не удалось обойти ${mods}: $($_.Exception.Message)" }
    foreach ($f in $files) {
        if ($zips.Count -ge $MaxZips) { Say "  архивов больше $MaxZips, остальные пропущены"; break }
        $name = [System.IO.Path]::GetFileName($f)
        if ($name -notmatch '\.zip$') { continue }
        $fi = New-Object System.IO.FileInfo($f)
        $z = [ordered]@{ path = (Rel $u $f); size = $fi.Length; lastWriteUtc = $fi.LastWriteTimeUtc.ToString('o'); nameHasNonAscii = ($name -match '[^\x20-\x7E]'); fullPathLength = $f.Length }
        if ($gameRunning -or $LockTest) {
            # Проверка «файл занят игрой»: открываем только на чтение и сразу закрываем.
            try { $s = [System.IO.File]::Open($f, 'Open', 'Read', 'None'); $s.Close(); $z.lockedByGame = $false }
            catch {
                $ex = $_.Exception; if ($ex.InnerException) { $ex = $ex.InnerException }
                $z.lockedByGame = $true; $z.lockError = $ex.Message; $z.lockHResult = ('0x{0:X8}' -f $ex.HResult)
            }
        }
        try {
            $arc = [System.IO.Compression.ZipFile]::OpenRead($f)
            try {
                $entries = $arc.Entries
                $z.entries = $entries.Count
                $z.uncompressed = ($entries | Measure-Object -Property Length -Sum).Sum
                $top = @{}; $veh = @{}; $lev = @{}; $ids = @{}; $imgs = @(); $ext = @{}
                $infos = @()
                foreach ($e in $entries) {
                    $p = $e.FullName -replace '\\', '/'
                    $parts = $p.Split('/')
                    $top[$parts[0]] = $true
                    if ($parts.Length -ge 3 -and $parts[0] -ieq 'vehicles') { $veh[$parts[1]] = $true }
                    if ($parts.Length -ge 3 -and $parts[0] -ieq 'levels') { $lev[$parts[1]] = $true }
                    if ($parts.Length -ge 3 -and $parts[0] -ieq 'mod_info') { $ids[$parts[1]] = $true }
                    $x = [System.IO.Path]::GetExtension($p).ToLowerInvariant()
                    if ($x) { $ext[$x] = 1 + [int]$ext[$x] }
                    if ($x -match '^\.(png|jpg|jpeg|webp|dds)$' -and $imgs.Count -lt 15 -and $x -ne '.dds') { $imgs += $p }
                    if ($p -imatch '^mod_info/[^/]+/info\.json$' -and $infos.Count -lt 3 -and $e.Length -lt 64KB) {
                        $r = New-Object System.IO.StreamReader($e.Open()); try { $infos += $r.ReadToEnd() } finally { $r.Close() }
                    }
                }
                $z.topLevel = @($top.Keys | Sort-Object)
                $z.vehicles = @($veh.Keys | Sort-Object)
                $z.levels = @($lev.Keys | Sort-Object)
                $z.modInfoIds = @($ids.Keys | Sort-Object)
                $z.modInfoJson = $infos
                $z.images = $imgs
                $z.extensions = $ext
            } finally { $arc.Dispose() }
        } catch { $z.zipError = $_.Exception.Message }
        $zips += $z
    }
}
SaveJson 'zips.json' $zips
$locked = @($zips | Where-Object { $_.lockedByGame }).Count
$broken = @($zips | Where-Object { $_.zipError }).Count
Say "Архивов: $($zips.Count), не открылись: $broken$(if ($gameRunning) { ", заняты игрой: $locked" })"
$dup = $zips | ForEach-Object { $zz = $_; $zz.vehicles | ForEach-Object { [pscustomobject]@{ v = $_; zip = $zz.path } } } | Group-Object v | Where-Object { $_.Count -gt 1 }
foreach ($g in $dup) { Say "  одна машина в нескольких архивах: $($g.Name) — $(($g.Group | ForEach-Object { $_.zip }) -join ', ')" }

# ---------------------------------------------------------------- 5. Архив
Write-Host "5/5 Упаковка..."
SaveText 'summary.txt' ($summary -join "`r`n")
$zipOut = "$work.zip"
try {
    if ([System.IO.File]::Exists($zipOut)) { [System.IO.File]::Delete($zipOut) }
    [System.IO.Compression.ZipFile]::CreateFromDirectory($work, $zipOut)
    Write-Host ""
    Write-Host "Готово. Пришлите этот файл:" -ForegroundColor Green
    Write-Host "  $zipOut" -ForegroundColor Green
} catch {
    Write-Host "Не удалось упаковать ($($_.Exception.Message)). Пришлите папку целиком: $work" -ForegroundColor Yellow
}
