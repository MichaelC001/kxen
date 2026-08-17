[CmdletBinding()]
param(
    [ValidateSet("All", "Server", "Agent")]
    [string]$Component = "All",

    [string]$Version = "latest",

    [string]$InstallDir = "",

    [switch]$NoModifyPath,

    [switch]$Help
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = "Stop"

$script:KxenRepository = "StringKe/kxen"
$script:KxenInstallerChecksumUri = "https://raw.githubusercontent.com/StringKe/kxen/main/website/public/install.ps1.sha256"
$script:KxenInstallerSourcePath = $PSCommandPath

function Show-KxenInstallHelp {
    @"
kxen headless CLI installer

USAGE:
    install.ps1 [-Component All|Server|Agent] [-Version latest|x.y.z]
                [-InstallDir PATH] [-NoModifyPath] [-Help]

OPTIONS:
    -Component       install both CLIs or one component (default: All)
    -Version         install the latest stable release or an exact version
    -InstallDir      binary directory (default: %LOCALAPPDATA%\Kxen\bin)
    -NoModifyPath    do not add the install directory to the user PATH
    -Help            print this help

INSTALLED COMMANDS:
    Server -> kxen.exe
    Agent  -> kxen-agent.exe
"@
}

function Test-KxenStableTag {
    param([Parameter(Mandatory = $true)][string]$Tag)
    return $Tag -match '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'
}

function ConvertTo-KxenStableTag {
    param([Parameter(Mandatory = $true)][string]$RequestedVersion)
    $tag = if ($RequestedVersion.StartsWith("v", [System.StringComparison]::Ordinal)) {
        $RequestedVersion
    }
    else {
        "v$RequestedVersion"
    }
    if (-not (Test-KxenStableTag -Tag $tag)) {
        throw "invalid stable version: $RequestedVersion"
    }
    return $tag
}

function Invoke-KxenDownload {
    param(
        [Parameter(Mandatory = $true)][string]$Uri,
        [Parameter(Mandatory = $true)][string]$OutFile
    )
    $headers = @{
        Accept = "application/vnd.github+json"
        "User-Agent" = "kxen-installer"
        "X-GitHub-Api-Version" = "2022-11-28"
    }
    Invoke-WebRequest -Uri $Uri -OutFile $OutFile -Headers $headers -UseBasicParsing | Out-Null
}

function Resolve-KxenReleaseTag {
    param(
        [Parameter(Mandatory = $true)][string]$RequestedVersion,
        [Parameter(Mandatory = $true)][string]$TemporaryDirectory
    )
    if ($RequestedVersion -ne "latest") {
        return ConvertTo-KxenStableTag -RequestedVersion $RequestedVersion
    }
    $releaseJson = Join-Path $TemporaryDirectory "latest-release.json"
    Invoke-KxenDownload `
        -Uri "https://api.github.com/repos/$script:KxenRepository/releases/latest" `
        -OutFile $releaseJson
    $release = Get-Content -LiteralPath $releaseJson -Raw | ConvertFrom-Json
    if ($null -eq $release.tag_name) {
        throw "GitHub latest release did not return tag_name"
    }
    $tag = [string]$release.tag_name
    if (-not (Test-KxenStableTag -Tag $tag)) {
        throw "GitHub latest release did not return a stable SemVer tag"
    }
    return $tag
}

function ConvertTo-KxenArchitecture {
    param([Parameter(Mandatory = $true)][string]$ArchitectureName)
    switch ($ArchitectureName.ToUpperInvariant()) {
        "X64" { return "x86_64" }
        "AMD64" { return "x86_64" }
        "ARM64" { return "aarch64" }
        default { throw "unsupported Windows architecture: $ArchitectureName" }
    }
}

function Get-KxenWindowsArchitecture {
    $architectureName = $null
    try {
        $architectureName = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    }
    catch {
        if (-not [string]::IsNullOrWhiteSpace($env:PROCESSOR_ARCHITEW6432)) {
            $architectureName = $env:PROCESSOR_ARCHITEW6432
        }
        else {
            $architectureName = $env:PROCESSOR_ARCHITECTURE
        }
    }
    if ([string]::IsNullOrWhiteSpace($architectureName)) {
        throw "unable to determine the Windows architecture"
    }
    return ConvertTo-KxenArchitecture -ArchitectureName $architectureName
}

function Get-KxenAssetName {
    param(
        [Parameter(Mandatory = $true)][ValidateSet("Server", "Agent")][string]$RequestedComponent,
        [Parameter(Mandatory = $true)][ValidateSet("x86_64", "aarch64")][string]$Architecture
    )
    if ($RequestedComponent -eq "Server") {
        return "kxen-windows-$Architecture.zip"
    }
    return "kxen-agent-windows-$Architecture.zip"
}

function Get-KxenBinaryName {
    param([Parameter(Mandatory = $true)][ValidateSet("Server", "Agent")][string]$RequestedComponent)
    if ($RequestedComponent -eq "Server") {
        return "kxen.exe"
    }
    return "kxen-agent.exe"
}

function Get-KxenExpectedChecksum {
    param(
        [Parameter(Mandatory = $true)][string]$ChecksumsPath,
        [Parameter(Mandatory = $true)][string]$AssetName
    )
    $escapedAsset = [System.Text.RegularExpressions.Regex]::Escape($AssetName)
    $pattern = "^([0-9A-Fa-f]{64})\s+\*?$escapedAsset$"
    $checksumMatches = @(
        Get-Content -LiteralPath $ChecksumsPath | ForEach-Object {
            if ($_ -match $pattern) {
                $Matches[1].ToLowerInvariant()
            }
        }
    )
    if ($checksumMatches.Count -ne 1) {
        throw "SHA256SUMS does not contain exactly one valid entry for $AssetName"
    }
    return $checksumMatches[0]
}

function Test-KxenChecksum {
    param(
        [Parameter(Mandatory = $true)][string]$ChecksumsPath,
        [Parameter(Mandatory = $true)][string]$AssetName,
        [Parameter(Mandatory = $true)][string]$AssetPath
    )
    $expected = Get-KxenExpectedChecksum -ChecksumsPath $ChecksumsPath -AssetName $AssetName
    $actual = (Get-FileHash -LiteralPath $AssetPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $expected) {
        throw "SHA-256 mismatch for $AssetName"
    }
    Write-Output "PASS SHA-256 $AssetName"
}

function Test-KxenInstallerIntegrity {
    param(
        [AllowEmptyString()][string]$SourcePath,
        [Parameter(Mandatory = $true)][string]$TemporaryDirectory
    )
    if ([string]::IsNullOrWhiteSpace($SourcePath) -or -not (Test-Path -LiteralPath $SourcePath -PathType Leaf)) {
        Write-Warning "installer self-check unavailable for piped input; HTTPS is the only script integrity boundary"
        return
    }
    $checksumsPath = Join-Path $TemporaryDirectory "install.ps1.sha256"
    Invoke-KxenDownload -Uri $script:KxenInstallerChecksumUri -OutFile $checksumsPath
    Test-KxenChecksum -ChecksumsPath $checksumsPath -AssetName "install.ps1" -AssetPath $SourcePath
}

function Expand-KxenAsset {
    param(
        [Parameter(Mandatory = $true)][string]$ArchivePath,
        [Parameter(Mandatory = $true)][string]$BinaryName,
        [Parameter(Mandatory = $true)][string]$OutputDirectory
    )
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [System.IO.Compression.ZipFile]::OpenRead($ArchivePath)
    try {
        $entries = @($archive.Entries)
        if ($entries.Count -ne 1 -or $entries[0].FullName -ne $BinaryName -or $entries[0].Name -ne $BinaryName) {
            throw "archive must contain only ${BinaryName}: $([System.IO.Path]::GetFileName($ArchivePath))"
        }
    }
    finally {
        $archive.Dispose()
    }
    New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
    [System.IO.Compression.ZipFile]::ExtractToDirectory($ArchivePath, $OutputDirectory)
    $binaryPath = Join-Path $OutputDirectory $BinaryName
    if (-not (Test-Path -LiteralPath $binaryPath -PathType Leaf)) {
        throw "archive did not produce a regular $BinaryName file"
    }
    return $binaryPath
}

function Test-KxenBinary {
    param(
        [Parameter(Mandatory = $true)][ValidateSet("Server", "Agent")][string]$RequestedComponent,
        [Parameter(Mandatory = $true)][string]$BinaryPath,
        [Parameter(Mandatory = $true)][string]$ReleaseTag
    )
    if ($RequestedComponent -eq "Server") {
        $null = & $BinaryPath --help 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "kxen --help failed with exit code $LASTEXITCODE"
        }
        return
    }
    $versionOutput = ((& $BinaryPath --version 2>&1 | Out-String).Trim())
    if ($LASTEXITCODE -ne 0) {
        throw "kxen-agent --version failed with exit code $LASTEXITCODE"
    }
    $expected = "kxen-agent $($ReleaseTag.Substring(1))"
    if ($versionOutput -ne $expected) {
        throw "kxen-agent version mismatch: expected $expected, got $versionOutput"
    }
}

function Test-KxenPathContains {
    param(
        [AllowNull()][string]$PathValue,
        [Parameter(Mandatory = $true)][string]$Directory
    )
    if ([string]::IsNullOrWhiteSpace($PathValue)) {
        return $false
    }
    $target = $Directory.TrimEnd('\')
    foreach ($entry in $PathValue.Split(';')) {
        if ($entry.Trim().TrimEnd('\').Equals($target, [System.StringComparison]::OrdinalIgnoreCase)) {
            return $true
        }
    }
    return $false
}

function Add-KxenToUserPath {
    param(
        [Parameter(Mandatory = $true)][string]$Directory,
        [Parameter(Mandatory = $true)][bool]$ModifyPath
    )
    if (-not $ModifyPath) {
        Write-Output "PATH not modified. Add this directory to your user PATH: $Directory"
        return
    }
    if (-not (Test-KxenPathContains -PathValue $env:Path -Directory $Directory)) {
        $env:Path = "$Directory;$env:Path"
    }
    $userPath = [System.Environment]::GetEnvironmentVariable("Path", "User")
    if (-not (Test-KxenPathContains -PathValue $userPath -Directory $Directory)) {
        $updatedUserPath = if ([string]::IsNullOrWhiteSpace($userPath)) {
            $Directory
        }
        else {
            "$Directory;$userPath"
        }
        [System.Environment]::SetEnvironmentVariable("Path", $updatedUserPath, "User")
        Write-Output "PASS added $Directory to the user PATH"
    }
    else {
        Write-Output "PASS user PATH already contains $Directory"
    }
    Write-Output "The commands are available in this PowerShell session and new terminals."
}

function Invoke-KxenInstall {
    param(
        [Parameter(Mandatory = $true)][ValidateSet("All", "Server", "Agent")][string]$RequestedComponent,
        [Parameter(Mandatory = $true)][string]$RequestedVersion,
        [string]$RequestedInstallDir,
        [Parameter(Mandatory = $true)][bool]$ModifyPath
    )
    if ($env:OS -ne "Windows_NT") {
        throw "native Windows installation requires Windows; macOS and Linux use https://kxen.ai/install.sh"
    }
    if ([string]::IsNullOrWhiteSpace($RequestedInstallDir)) {
        $localApplicationData = [System.Environment]::GetFolderPath("LocalApplicationData")
        if ([string]::IsNullOrWhiteSpace($localApplicationData)) {
            throw "unable to determine LocalApplicationData; pass -InstallDir"
        }
        $RequestedInstallDir = Join-Path (Join-Path $localApplicationData "Kxen") "bin"
    }
    if (-not [System.IO.Path]::IsPathRooted($RequestedInstallDir)) {
        throw "-InstallDir must be absolute: $RequestedInstallDir"
    }
    $resolvedInstallDir = [System.IO.Path]::GetFullPath($RequestedInstallDir)
    $temporaryDirectory = Join-Path ([System.IO.Path]::GetTempPath()) ("kxen-install-" + [System.Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $temporaryDirectory | Out-Null
    $transactionActive = $false
    $succeeded = $false
    $records = @()
    try {
        Test-KxenInstallerIntegrity `
            -SourcePath $script:KxenInstallerSourcePath `
            -TemporaryDirectory $temporaryDirectory
        $architecture = Get-KxenWindowsArchitecture
        $releaseTag = Resolve-KxenReleaseTag -RequestedVersion $RequestedVersion -TemporaryDirectory $temporaryDirectory
        $components = if ($RequestedComponent -eq "All") { @("Server", "Agent") } else { @($RequestedComponent) }
        $releaseBase = "https://github.com/$script:KxenRepository/releases/download/$releaseTag"
        $checksumsPath = Join-Path $temporaryDirectory "SHA256SUMS"
        Invoke-KxenDownload -Uri "$releaseBase/SHA256SUMS" -OutFile $checksumsPath
        Write-Output "Installing Kxen $releaseTag for windows-$architecture into $resolvedInstallDir"

        $staged = @()
        foreach ($selectedComponent in $components) {
            $assetName = Get-KxenAssetName -RequestedComponent $selectedComponent -Architecture $architecture
            $binaryName = Get-KxenBinaryName -RequestedComponent $selectedComponent
            $archivePath = Join-Path $temporaryDirectory $assetName
            $extractDirectory = Join-Path (Join-Path $temporaryDirectory "staged") $selectedComponent.ToLowerInvariant()
            Invoke-KxenDownload -Uri "$releaseBase/$assetName" -OutFile $archivePath
            Test-KxenChecksum -ChecksumsPath $checksumsPath -AssetName $assetName -AssetPath $archivePath
            $binaryPath = Expand-KxenAsset -ArchivePath $archivePath -BinaryName $binaryName -OutputDirectory $extractDirectory
            Test-KxenBinary -RequestedComponent $selectedComponent -BinaryPath $binaryPath -ReleaseTag $releaseTag
            Write-Output "PASS verified $binaryName"
            $staged += [PSCustomObject]@{
                Component = $selectedComponent
                BinaryName = $binaryName
                Source = $binaryPath
            }
        }

        New-Item -ItemType Directory -Path $resolvedInstallDir -Force | Out-Null
        foreach ($item in $staged) {
            $destination = Join-Path $resolvedInstallDir $item.BinaryName
            if (Test-Path -LiteralPath $destination -PathType Container) {
                throw "destination is a directory: $destination"
            }
            $unique = [System.Guid]::NewGuid().ToString("N")
            $pending = Join-Path $resolvedInstallDir ".$($item.BinaryName).kxen-install.$unique"
            $backup = "$pending.backup"
            $record = [PSCustomObject]@{
                Destination = $destination
                Pending = $pending
                Backup = $backup
                BackupMoved = $false
                Installed = $false
            }
            $records += $record
            Copy-Item -LiteralPath $item.Source -Destination $pending
        }

        $transactionActive = $true
        foreach ($record in $records) {
            if (Test-Path -LiteralPath $record.Destination) {
                Move-Item -LiteralPath $record.Destination -Destination $record.Backup
                $record.BackupMoved = $true
            }
            Move-Item -LiteralPath $record.Pending -Destination $record.Destination
            $record.Installed = $true
        }

        Add-KxenToUserPath -Directory $resolvedInstallDir -ModifyPath $ModifyPath

        $transactionActive = $false
        $succeeded = $true
        foreach ($item in $staged) {
            Write-Output "PASS installed $(Join-Path $resolvedInstallDir $item.BinaryName)"
        }
    }
    catch {
        if ($transactionActive) {
            for ($index = $records.Count - 1; $index -ge 0; $index--) {
                $record = $records[$index]
                if ($record.Installed -and (Test-Path -LiteralPath $record.Destination)) {
                    Move-Item -LiteralPath $record.Destination -Destination (Join-Path $temporaryDirectory ([System.Guid]::NewGuid().ToString("N"))) -ErrorAction SilentlyContinue
                }
                if ($record.BackupMoved -and (Test-Path -LiteralPath $record.Backup)) {
                    Move-Item -LiteralPath $record.Backup -Destination $record.Destination -ErrorAction SilentlyContinue
                }
                if (Test-Path -LiteralPath $record.Pending) {
                    Move-Item -LiteralPath $record.Pending -Destination (Join-Path $temporaryDirectory ([System.Guid]::NewGuid().ToString("N"))) -ErrorAction SilentlyContinue
                }
            }
        }
        throw "Kxen installation failed: $($_.Exception.Message)"
    }
    finally {
        foreach ($record in $records) {
            if (Test-Path -LiteralPath $record.Pending) {
                Remove-Item -LiteralPath $record.Pending -Force -ErrorAction SilentlyContinue
            }
            if ($succeeded -and (Test-Path -LiteralPath $record.Backup)) {
                Remove-Item -LiteralPath $record.Backup -Force -ErrorAction SilentlyContinue
            }
        }
        if (Test-Path -LiteralPath $temporaryDirectory) {
            Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force
        }
    }
}

if ($MyInvocation.InvocationName -ne '.') {
    if ($Help) {
        Show-KxenInstallHelp
    }
    else {
        Invoke-KxenInstall `
            -RequestedComponent $Component `
            -RequestedVersion $Version `
            -RequestedInstallDir $InstallDir `
            -ModifyPath (-not $NoModifyPath)
    }
}
