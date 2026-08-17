Set-StrictMode -Version 2.0
$ErrorActionPreference = "Stop"

$script:Failures = 0
$script:RepoDirectory = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
. (Join-Path $script:RepoDirectory "website/public/install.ps1")

function Add-TestFailure {
    param([Parameter(Mandatory = $true)][string]$Message)
    Write-Error "FAIL $Message" -ErrorAction Continue
    $script:Failures += 1
}

function Assert-Equal {
    param(
        [Parameter(Mandatory = $true)][string]$Label,
        [AllowNull()]$Expected,
        [AllowNull()]$Actual
    )
    if ($Expected -ne $Actual) {
        Add-TestFailure "$Label expected '$Expected', got '$Actual'"
    }
}

Assert-Equal "normalize plain version" "v1.2.3" (ConvertTo-KxenStableTag -RequestedVersion "1.2.3")
Assert-Equal "normalize prefixed version" "v1.2.3" (ConvertTo-KxenStableTag -RequestedVersion "v1.2.3")
foreach ($invalidVersion in @("1.2", "v01.2.3", "1.2.3-beta", "latest", "")) {
    try {
        $null = ConvertTo-KxenStableTag -RequestedVersion $invalidVersion
        Add-TestFailure "invalid version was accepted: '$invalidVersion'"
    }
    catch {
    }
}
Assert-Equal "x64 architecture" "x86_64" (ConvertTo-KxenArchitecture -ArchitectureName "AMD64")
Assert-Equal "ARM64 architecture" "aarch64" (ConvertTo-KxenArchitecture -ArchitectureName "ARM64")
Assert-Equal "x64 server asset" "kxen-windows-x86_64.zip" (Get-KxenAssetName -RequestedComponent Server -Architecture x86_64)
Assert-Equal "ARM64 agent asset" "kxen-agent-windows-aarch64.zip" (Get-KxenAssetName -RequestedComponent Agent -Architecture aarch64)
if (-not (Test-KxenPathContains -PathValue "C:\one;C:\Kxen\bin" -Directory "c:\kxen\bin\")) {
    Add-TestFailure "PATH comparison was not case-insensitive"
}

$testDirectory = Join-Path ([System.IO.Path]::GetTempPath()) ("kxen-installers-test-" + [System.Guid]::NewGuid().ToString("N"))
$script:FixtureDirectory = Join-Path $testDirectory "fixture"
$originalOs = $env:OS
New-Item -ItemType Directory -Path $script:FixtureDirectory | Out-Null
try {
    # Fixture 不执行 PE 文件，固定 OS 只用于覆盖 Windows 安装事务，可在任意 pwsh runner 重放。
    $env:OS = "Windows_NT"
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    foreach ($fixture in @(
        @{ Directory = "server"; Binary = "kxen.exe"; Asset = "kxen-windows-x86_64.zip" },
        @{ Directory = "agent"; Binary = "kxen-agent.exe"; Asset = "kxen-agent-windows-x86_64.zip" }
    )) {
        $sourceDirectory = Join-Path $script:FixtureDirectory $fixture.Directory
        New-Item -ItemType Directory -Path $sourceDirectory | Out-Null
        [System.IO.File]::WriteAllText((Join-Path $sourceDirectory $fixture.Binary), "fixture-$($fixture.Binary)")
        [System.IO.Compression.ZipFile]::CreateFromDirectory($sourceDirectory, (Join-Path $script:FixtureDirectory $fixture.Asset))
    }
    $checksumLines = @()
    foreach ($assetName in @("kxen-windows-x86_64.zip", "kxen-agent-windows-x86_64.zip")) {
        $assetPath = Join-Path $script:FixtureDirectory $assetName
        $hash = (Get-FileHash -LiteralPath $assetPath -Algorithm SHA256).Hash.ToLowerInvariant()
        $checksumLines += "$hash  $assetName"
    }
    [System.IO.File]::WriteAllLines((Join-Path $script:FixtureDirectory "SHA256SUMS"), $checksumLines)
    $installerHash = (Get-FileHash -LiteralPath (Join-Path $script:RepoDirectory "website/public/install.ps1") -Algorithm SHA256).Hash.ToLowerInvariant()
    [System.IO.File]::WriteAllText(
        (Join-Path $script:FixtureDirectory "install.ps1.sha256"),
        "$installerHash  install.ps1`n"
    )

    function Invoke-KxenDownload {
        param(
            [Parameter(Mandatory = $true)][string]$Uri,
            [Parameter(Mandatory = $true)][string]$OutFile
        )
        $assetName = [System.IO.Path]::GetFileName(([System.Uri]$Uri).AbsolutePath)
        Copy-Item -LiteralPath (Join-Path $script:FixtureDirectory $assetName) -Destination $OutFile
    }
    function Get-KxenWindowsArchitecture {
        return "x86_64"
    }
    function Test-KxenBinary {
        param(
            [Parameter(Mandatory = $true)][string]$RequestedComponent,
            [Parameter(Mandatory = $true)][string]$BinaryPath,
            [Parameter(Mandatory = $true)][string]$ReleaseTag
        )
        if (-not (Test-Path -LiteralPath $BinaryPath -PathType Leaf) -or $ReleaseTag -ne "v9.8.7") {
            throw "invalid fixture binary"
        }
    }

    $allDirectory = Join-Path $testDirectory "install all"
    Invoke-KxenInstall -RequestedComponent All -RequestedVersion "9.8.7" -RequestedInstallDir $allDirectory -ModifyPath $false | Out-Null
    if (-not (Test-Path -LiteralPath (Join-Path $allDirectory "kxen.exe") -PathType Leaf) -or
        -not (Test-Path -LiteralPath (Join-Path $allDirectory "kxen-agent.exe") -PathType Leaf)) {
        Add-TestFailure "default all installation did not install both executables"
    }

    $agentDirectory = Join-Path $testDirectory "install-agent"
    Invoke-KxenInstall -RequestedComponent Agent -RequestedVersion "9.8.7" -RequestedInstallDir $agentDirectory -ModifyPath $false | Out-Null
    if (-not (Test-Path -LiteralPath (Join-Path $agentDirectory "kxen-agent.exe") -PathType Leaf) -or
        (Test-Path -LiteralPath (Join-Path $agentDirectory "kxen.exe"))) {
        Add-TestFailure "agent-only installation selected the wrong executables"
    }

    $installerSidecar = Join-Path $script:FixtureDirectory "install.ps1.sha256"
    $validInstallerSidecar = [System.IO.File]::ReadAllText($installerSidecar)
    $selfCheckDirectory = Join-Path $testDirectory "install-self-check"
    try {
        [System.IO.File]::WriteAllText($installerSidecar, ("0" * 64) + "  install.ps1`n")
        try {
            Invoke-KxenInstall -RequestedComponent All -RequestedVersion "9.8.7" -RequestedInstallDir $selfCheckDirectory -ModifyPath $false | Out-Null
            Add-TestFailure "installer self-check mismatch was accepted"
        }
        catch {
        }
        if ((Test-Path -LiteralPath (Join-Path $selfCheckDirectory "kxen.exe")) -or
            (Test-Path -LiteralPath (Join-Path $selfCheckDirectory "kxen-agent.exe"))) {
            Add-TestFailure "installer self-check failure changed the installation directory"
        }
    }
    finally {
        [System.IO.File]::WriteAllText($installerSidecar, $validInstallerSidecar)
    }

    $rollbackDirectory = Join-Path $testDirectory "install-rollback"
    New-Item -ItemType Directory -Path (Join-Path $rollbackDirectory "kxen-agent.exe") -Force | Out-Null
    [System.IO.File]::WriteAllText((Join-Path $rollbackDirectory "kxen.exe"), "old-server")
    try {
        Invoke-KxenInstall -RequestedComponent All -RequestedVersion "9.8.7" -RequestedInstallDir $rollbackDirectory -ModifyPath $false | Out-Null
        Add-TestFailure "directory destination was accepted"
    }
    catch {
    }
    Assert-Equal "preflight failure preserved server" "old-server" ([System.IO.File]::ReadAllText((Join-Path $rollbackDirectory "kxen.exe")))
    $pending = @(Get-ChildItem -LiteralPath $rollbackDirectory -Filter "*.kxen-install.*" -Force)
    if ($pending.Count -ne 0) {
        Add-TestFailure "preflight failure left an installer-owned pending file"
    }

    $extraDirectory = Join-Path $testDirectory "extra"
    New-Item -ItemType Directory -Path $extraDirectory | Out-Null
    [System.IO.File]::WriteAllText((Join-Path $extraDirectory "kxen.exe"), "one")
    [System.IO.File]::WriteAllText((Join-Path $extraDirectory "unexpected"), "two")
    $extraArchive = Join-Path $testDirectory "extra.zip"
    [System.IO.Compression.ZipFile]::CreateFromDirectory($extraDirectory, $extraArchive)
    try {
        $null = Expand-KxenAsset -ArchivePath $extraArchive -BinaryName "kxen.exe" -OutputDirectory (Join-Path $testDirectory "extra-out")
        Add-TestFailure "archive with an extra entry was accepted"
    }
    catch {
    }
}
finally {
    $env:OS = $originalOs
    if (Test-Path -LiteralPath $testDirectory) {
        Remove-Item -LiteralPath $testDirectory -Recurse -Force
    }
}

if ($script:Failures -ne 0) {
    throw "installer tests failed: $script:Failures failure(s)"
}
Write-Output "PASS Windows installer tests"
