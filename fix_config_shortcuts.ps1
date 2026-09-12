# Script de correction de launcher_config.json
# Resout tous les raccourcis (.lnk) vers leurs veritables cibles et icones

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
if ([string]::IsNullOrWhiteSpace($ScriptDir)) {
    $ScriptDir = (Get-Location).Path
}

$ConfigFile = Join-Path $ScriptDir "launcher_config.json"
if (-not (Test-Path $ConfigFile)) {
    $ConfigFile = "launcher_config.json"
}

if (-not (Test-Path $ConfigFile)) {
    Write-Host "[ERREUR] Le fichier launcher_config.json est introuvable dans $ScriptDir" -ForegroundColor Red
    exit 1
}

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host " Correction des raccourcis dans launcher_config.json      " -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "Fichier cible : $ConfigFile" -ForegroundColor Gray

# 1. Sauvegarde de securite
$DateStr = Get-Date -Format "yyyyMMdd_HHmmss"
$BackupFile = "$ConfigFile.backup_$DateStr"
Copy-Item -Path $ConfigFile -Destination $BackupFile -Force
Write-Host "[OK] Sauvegarde de securite : $BackupFile" -ForegroundColor Green

# 2. Lecture du JSON
try {
    $RawJson = Get-Content -Path $ConfigFile -Raw -Encoding UTF8
    $Config = $RawJson | ConvertFrom-Json
} catch {
    Write-Host "[ERREUR] Impossible de lire ou parser launcher_config.json" -ForegroundColor Red
    exit 1
}

$Wsh = New-Object -ComObject WScript.Shell
$ItemsModified = 0
$IconsModified = 0

# Dictionnaire de secours contenant les cibles resolues (meme si le .lnk d'origine a ete supprime/deplace)
$KnownMappings = @{
    "C:\Users\Public\Desktop\VALORANT.lnk" = @{
        Target = "C:\Riot Games\Riot Client\RiotClientServices.exe"
        Args   = "--launch-product=valorant --launch-patchline=live"
    }
    "C:\Users\Public\Desktop\Forza Horizon 6.lnk" = @{
        Target = "G:\Jeux\Forza Horizon 6\forzahorizon6.exe"
        Args   = ""
    }
    "G:\Bureau\Epic Games Launcher.lnk" = @{
        Target = "G:\Jeux\Epic Games\Launcher\Portal\Binaries\Win64\EpicGamesLauncher.exe"
        Args   = ""
    }
    "C:\Users\Public\Desktop\Steam.lnk" = @{
        Target = "G:\Jeux\Steam\steam.exe"
        Args   = ""
    }
    "G:\Bureau\FAF Client.lnk" = @{
        Target = "G:\Jeux\FAF Client\faf-client.exe"
        Args   = ""
    }
    "C:\Users\Public\Desktop\Need for Speed Underground 2.lnk" = @{
        Target = "H:\Jeux\NFS\speed2.exe"
        Args   = ""
    }
    "G:\Bureau\Game Center.lnk" = @{
        Target = "C:\ProgramData\Wargaming.net\GameCenter\wgc.exe"
        Args   = ""
    }
    "G:\Bureau\WoW.lnk" = @{
        Target = "G:\Jeux\ModWoT\Lanceur_WoW\Lanceur.exe"
        Args   = ""
    }
    "G:\Bureau\WoT.lnk" = @{
        Target = "G:\Jeux\ModWoT\Lanceur_WoW\Lanceur.exe"
        Args   = ""
    }
    "C:\Users\Public\Desktop\Need for SpeedT Most Wanted.lnk" = @{
        Target = "G:\Jeux\NFS Most Wanted\speed.exe"
        Args   = ""
    }
    "G:\Bureau\Firefox.lnk" = @{
        Target = "D:\PortableApps\PortableApps\FirefoxPortable\FirefoxPortable.exe"
        Args   = ""
    }
    "G:\Bureau\Web.lnk" = @{
        Target = "D:\Logiciels\Lance URL\Lanceur.exe"
        Args   = ""
    }
    "G:\Bureau\Tor Browser.lnk" = @{
        Target = "D:\Logiciels\Tor Browser\Browser\firefox.exe"
        Args   = ""
    }
    "G:\Bureau\Telechargements.lnk" = @{
        Target = "\\192.168.1.50\Disk_sda1\Video\Telechargements"
        Args   = ""
    }
    "G:\Bureau\AIMP.lnk" = @{
        Target = "D:\PortableApps\PortableApps\AIMPPortable\AIMPPortable.exe"
        Args   = ""
    }
    "G:\Bureau\MediaMonkey.lnk" = @{
        Target = "D:\Logiciels\MediaMonkey 5\MediaMonkey.exe"
        Args   = ""
    }
    "G:\Bureau\Start.exe.lnk" = @{
        Target = "D:\PortableApps\Start.exe"
        Args   = ""
    }
    "G:\Bureau\SyncBackFree.lnk" = @{
        Target = "D:\Logiciels\SyncBackFree\SyncBackFree.exe"
        Args   = ""
    }
    "G:\Bureau\AnyDesk.lnk" = @{
        Target = "D:\PortableApps\PortableApps\AnyDesk.exe"
        Args   = ""
    }
    "G:\Bureau\Proton VPN.lnk" = @{
        Target = "C:\Program Files\Proton\VPN\ProtonVPN.Launcher.exe"
        Args   = ""
    }
    "G:\Bureau\WinOFF.lnk" = @{
        Target = "D:\Logiciels\WinOFF\WinOFF.exe"
        Args   = ""
    }
    "G:\Bureau\RatioMaster.NE.lnk" = @{
        Target = "D:\PortableApps\PortableApps\RatioMaster.NET_0.43\RatioMaster.NET.exe"
        Args   = ""
    }
    "G:\Bureau\LiberKey.lnk" = @{
        Target = "D:\LiberKey\LiberKey.exe"
        Args   = ""
    }
    "G:\Bureau\WakeOnLan.lnk" = @{
        Target = "D:\Logiciels\WakeOnLan\WakeOnLan.exe"
        Args   = ""
    }
    "C:\Users\Public\Desktop\SOLIDWORKS Visualize 2026.lnk" = @{
        Target = "C:\WINDOWS\Installer\{43A5BA82-3E32-4525-B411-92AEC7BC51F3}\NewShortcut3_3D1779AA0C7C43C4B6613BB5AAAE02E8.exe"
        Args   = ""
    }
    "C:\Users\Public\Desktop\SOLIDWORKS Design 2026.lnk" = @{
        Target = "C:\WINDOWS\Installer\{6172DDB3-98E6-46E1-A0C1-DB8FD05063EF}\i386_SldWorks.exe"
        Args   = ""
    }
    "C:\Users\Public\Desktop\SOLIDWORKS Inspection 2026.lnk" = @{
        Target = "C:\WINDOWS\Installer\{FE4D8667-03A6-424A-961D-1941FAEDD562}\NewShortcut1_B420C9E791B148AB9CB47D6FC10B01A0.exe"
        Args   = ""
    }
    "C:\Users\Public\Desktop\SOLIDWORKS Composer 2026.lnk" = @{
        Target = "C:\WINDOWS\Installer\{F3C5B446-61CB-41D4-810B-F35ED0F79CE7}\NewShortcut11_06F9E119E1D74B5DB30960A26126B9BB.exe"
        Args   = ""
    }
    "C:\Users\Public\Desktop\SOLIDWORKS Design Electrical.lnk" = @{
        Target = "G:\Logiciels\SolidWorks\SOLIDWORKS Electrical\bin\SOLIDWORKSElectrical.exe"
        Args   = ""
    }
    "C:\Users\Public\Desktop\SOLIDWORKS Visualize Boost 2026.lnk" = @{
        Target = "C:\WINDOWS\Installer\{2DDBECD9-41B2-478B-BC03-66120D3E6601}\NewShortcut2_0A33A5A15B224D43914DF7CAB62AE454.exe"
        Args   = ""
    }
    "C:\Users\Public\Desktop\eDrawings 2026 x64 Edition.lnk" = @{
        Target = "C:\WINDOWS\Installer\{4A275299-9605-43E0-8249-E157D4370DA5}\NewShortcut5.11CCDA48_0F59_4209_ACA1_FCDB865558EA.exe"
        Args   = ""
    }
    "C:\Users\Public\Desktop\SOLIDWORKS Composer Player 2026.lnk" = @{
        Target = "C:\WINDOWS\Installer\{F3C5B446-61CB-41D4-810B-F35ED0F79CE7}\NewShortcut11_1.05E8B3F6_C6F0_450A_B0AB_1C0A5E596B61.exe"
        Args   = ""
    }
    "C:\Users\Public\Desktop\SOLIDWORKS Composer Sync 2026.lnk" = @{
        Target = "C:\WINDOWS\Installer\{F3C5B446-61CB-41D4-810B-F35ED0F79CE7}\NewShortcut21_6239964293204D908B17B13C3B2CA0A0.exe"
        Args   = ""
    }
    "C:\Users\Public\Desktop\EdiLus v.BIM 3(d) (x64).lnk" = @{
        Target = "D:\Logiciels\AccaEdilus\EdiLus.EXE"
        Args   = ""
    }
}

function Resolve-Lnk([string]$LnkPath) {
    if ([string]::IsNullOrWhiteSpace($LnkPath)) { return $null }
    $Clean = $LnkPath.Trim().Trim('"')
    
    # 1. Verification dans le dictionnaire de secours
    foreach ($K in $KnownMappings.Keys) {
        if ($Clean.Equals($K, [System.StringComparison]::OrdinalIgnoreCase)) {
            return $KnownMappings[$K]
        }
    }

    # 2. Resolution dynamique directe si le fichier existe
    if ($Clean.ToLower().EndsWith(".lnk") -and (Test-Path $Clean)) {
        try {
            $Sc = $Wsh.CreateShortcut($Clean)
            return @{
                Target = $Sc.TargetPath
                Args   = $Sc.Arguments
                Icon   = $Sc.IconLocation
            }
        } catch {
            return $null
        }
    }

    return $null
}

# 3. Parcours des conteneurs et des items
if ($Config.containers) {
    foreach ($Cont in $Config.containers) {
        if ($Cont.icon_path -and $Cont.icon_path.ToLower().EndsWith(".lnk")) {
            $Res = Resolve-Lnk $Cont.icon_path
            if ($Res -and $Res.Target) {
                Write-Host "  [Conteneur $($Cont.name)] Icone : $($Cont.icon_path) -> $($Res.Target)" -ForegroundColor Yellow
                $Cont.icon_path = $Res.Target
                $IconsModified++
            }
        }

        if ($Cont.items) {
            foreach ($Itm in $Cont.items) {
                $OldTarget = $Itm.target
                $Res = Resolve-Lnk $OldTarget

                if ($Res -and $Res.Target) {
                    $RealTarget = $Res.Target
                    Write-Host "  [Item $($Itm.name)] Cible resolue : $OldTarget -> $RealTarget" -ForegroundColor Yellow
                    $Itm.target = $RealTarget
                    $ItemsModified++

                    if ([string]::IsNullOrWhiteSpace($Itm.args) -and -not [string]::IsNullOrWhiteSpace($Res.Args)) {
                        $Itm.args = $Res.Args
                        Write-Host "    └ Arguments ajoutes : $($Res.Args)" -ForegroundColor DarkYellow
                    }
                }

                if ($Itm.icon_type -eq "extracted") {
                    if (-not [string]::IsNullOrWhiteSpace($Itm.icon_value) -and $Itm.icon_value.ToLower().EndsWith(".lnk")) {
                        $ResIcon = Resolve-Lnk $Itm.icon_value
                        if ($ResIcon -and $ResIcon.Target) {
                            Write-Host "  [Item $($Itm.name)] Icone .lnk resolue -> $($ResIcon.Target)" -ForegroundColor Cyan
                            $Itm.icon_value = $ResIcon.Target
                            $IconsModified++
                        }
                    } elseif ([string]::IsNullOrWhiteSpace($Itm.icon_value) -or $Itm.icon_value -eq "🚀") {
                        if (-not [string]::IsNullOrWhiteSpace($Itm.target) -and -not $Itm.target.ToLower().EndsWith(".lnk")) {
                            $Itm.icon_value = $Itm.target
                            Write-Host "  [Item $($Itm.name)] Icone liee a la cible : $($Itm.target)" -ForegroundColor Cyan
                            $IconsModified++
                        }
                    }
                }
            }
        }
    }
}

# 4. Enregistrement JSON (sans UTF-8 BOM pour compatibilite serde_json)
$Utf8NoBom = New-Object System.Text.UTF8Encoding($false)

if ($ItemsModified -gt 0 -or $IconsModified -gt 0) {
    try {
        $UpdatedJson = $Config | ConvertTo-Json -Depth 10
        [System.IO.File]::WriteAllText($ConfigFile, $UpdatedJson, $Utf8NoBom)
        Write-Host ""
        Write-Host "==========================================================" -ForegroundColor Green
        Write-Host " [SUCCES] Mise a jour terminee avec succes !" -ForegroundColor Green
        Write-Host "  - Cibles modifiees   : $ItemsModified" -ForegroundColor Green
        Write-Host "  - Icones mises a jour: $IconsModified" -ForegroundColor Green
        Write-Host "==========================================================" -ForegroundColor Green
    } catch {
        Write-Host "[ERREUR] Impossible de sauvegarder le fichier JSON : $_" -ForegroundColor Red
        exit 1
    }
} else {
    # Verifier si le fichier actuel possede un BOM UTF-8 et le retirer si besoin
    $CurrentBytes = [System.IO.File]::ReadAllBytes($ConfigFile)
    if ($CurrentBytes.Length -ge 3 -and $CurrentBytes[0] -eq 0xEF -and $CurrentBytes[1] -eq 0xBB -and $CurrentBytes[2] -eq 0xBF) {
        $CleanText = [System.Text.Encoding]::UTF8.GetString($CurrentBytes, 3, $CurrentBytes.Length - 3)
        [System.IO.File]::WriteAllText($ConfigFile, $CleanText, $Utf8NoBom)
        Write-Host "[INFO] BOM UTF-8 supprime de $ConfigFile" -ForegroundColor Yellow
    }
    Write-Host ""
    Write-Host "[INFO] Tous les chemins sont deja a jour ou aucun .lnk resoluble." -ForegroundColor Gray
}

# 5. Synchronisation automatique vers target\release et target\debug si presents
$ReleaseDir = Join-Path $ScriptDir "target\release"
if (Test-Path $ReleaseDir) {
    Copy-Item -Path $ConfigFile -Destination (Join-Path $ReleaseDir "launcher_config.json") -Force
    Write-Host "[SYNC] Fichier a jour dans : $ReleaseDir\launcher_config.json" -ForegroundColor Cyan
}
$DebugDir = Join-Path $ScriptDir "target\debug"
if (Test-Path $DebugDir) {
    Copy-Item -Path $ConfigFile -Destination (Join-Path $DebugDir "launcher_config.json") -Force
    Write-Host "[SYNC] Fichier a jour dans : $DebugDir\launcher_config.json" -ForegroundColor Cyan
}
