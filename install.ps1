# Installs the newest bravebot release for Windows, or the one named by the version given.
#
#   irm https://raw.githubusercontent.com/brave/bravebot/main/install.ps1 | iex
#   & ([scriptblock]::Create((irm https://raw.githubusercontent.com/brave/bravebot/main/install.ps1))) 1.2.3
#   $env:INSTALL_DIR = "$HOME\bin"; irm https://raw.githubusercontent.com/brave/bravebot/main/install.ps1 | iex
#
# It needs no Administrator and no Node. The checksum check is not optional: without it a
# network-fetched executable would run on the strength of TLS alone, and a substituted release asset
# would be indistinguishable from a good one. Nor is the signature check. The checksum is published
# beside the binary, so whoever can replace one can replace both, and the binary's Authenticode
# signature has to be valid and made by Brave. Running this again is also how an install made this
# way is updated, so it writes down where the binary went: that is what lets bravebot name the
# command that updates this copy, and what puts a later run in the same place rather than beside it.

param([Parameter(Position = 0)][string]$Version = "")

# Everything runs inside this block. `iex` runs a script in the caller's own scope, so a preference
# set or a function defined at the top level would still be there when the person's prompt returns.
# A refusal is a thrown error and never `exit`, which would close the terminal this was pasted into.
& {
    param([string]$Requested, [bool]$Given)

    $ErrorActionPreference = "Stop"
    Set-StrictMode -Version 3.0
    # Windows PowerShell 5.1 draws a progress bar per download, which makes a large one many times slower.
    $ProgressPreference = "SilentlyContinue"
    # Windows PowerShell 5.1 can be left offering only TLS versions older than the one GitHub accepts.
    # A session with a list of its own that lacks 1.2 gets it added, and nothing is removed. A session on
    # the system default, which is the value 0, is left alone, since a list of one would narrow it.
    if ($PSVersionTable.PSVersion.Major -lt 6) {
        $protocols = [Net.ServicePointManager]::SecurityProtocol
        if ([int]$protocols -ne 0 -and -not ($protocols -band [Net.SecurityProtocolType]::Tls12)) {
            [Net.ServicePointManager]::SecurityProtocol = $protocols -bor [Net.SecurityProtocolType]::Tls12
        }
    }

    # Stated here and nowhere else, and composed from nothing: not an argument, not a file and not the
    # environment. The asset and its checksum come from the same release, so an origin something outside
    # this file could supply would move both halves of the comparison at once, and a substituted binary
    # published beside its own true digest would pass the check.
    $Repo = "brave/bravebot"
    $ApiUrl = "https://api.github.com/repos/$Repo/releases/latest"
    # Who the binary has to be signed by. Authenticode alone accepts a valid signature from any signer a
    # trusted authority issued a certificate to, so a signature from anyone else is a refusal.
    $SignerName = "Brave Software, Inc."
    $BinName = "bravebot.exe"

    function Fail([string]$Message) {
        throw "error: $Message"
    }

    # Three decimal numbers joined by dots, and nothing else. The version becomes part of a URL, so it is
    # held to this shape rather than to "does not look dangerous": a dot-dot, a slash or a host in it is
    # refused before any request, and what is left can only name a tag under the repository. The digits
    # are spelled out because .NET's \d also matches the digits of other scripts, and the ends are \A and
    # \z because $ also matches before a trailing newline.
    function Test-Version([string]$Text) {
        $Text -cmatch '\A[0-9]+\.[0-9]+\.[0-9]+\z'
    }

    # The digest and nothing else: sixty-four hex digits, no filename beside them. A checksum file of any
    # other shape is a release published wrong, and installing anyway would make the one signal that
    # distinguishes a bad download from a good one meaningless.
    function Test-Sha256([string]$Text) {
        $Text -cmatch '\A[0-9a-fA-F]{64}\z'
    }

    # Checked first, so an argument that is refused costs no request and writes nothing. An argument that
    # is given and empty is refused as well, since a person who typed one meant it to name something.
    $tag = ""
    if ($Given) {
        $wanted = $Requested
        if ($wanted.StartsWith("v", [StringComparison]::Ordinal)) {
            $wanted = $wanted.Substring(1)
        }
        if (-not (Test-Version $wanted)) {
            Fail "not a version: expected three numbers such as 1.2.3 (a leading v is allowed)"
        }
        $tag = "v$wanted"
    }

    # A PowerShell without this command is not on Windows, or is on one that cannot check what it is about
    # to install. A missing tool is refused rather than skipped, because an install that skipped the check
    # for want of the tool would be the one an attacker arranges.
    if (-not (Get-Command Get-AuthenticodeSignature -ErrorAction SilentlyContinue)) {
        Fail "Get-AuthenticodeSignature is not available in this PowerShell, so the download cannot be checked for Brave's signature; refusing to install. This script installs bravebot for Windows; macOS and Linux use install.sh"
    }

    # The architecture .NET reports for the machine. A PowerShell running under emulation can report the
    # emulated one, and then the binary for that architecture is installed and runs the same way.
    $native = ""
    try {
        $native = [string][System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
    } catch {
        $native = ""
    }
    if (-not $native) {
        $native = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { [string]$env:PROCESSOR_ARCHITECTURE }
    }
    $arch = switch ($native) {
        { $_ -in "X64", "AMD64" } { "amd64" }
        "Arm64" { "arm64" }
        default { Fail "unsupported architecture: $native" }
    }
    $asset = "bravebot-windows-$arch.exe"

    # An unset or empty profile directory names no state directory at all: nothing is read from one and
    # nothing written to one (docs/specs/state-directory.md#STATE-2). Joining the name onto nothing would
    # make a relative `.bravebot`, a location nobody stated, and a record under it would decide where this
    # release lands. HOME first, then USERPROFILE, which is the order the program reads them in.
    $profileDir = ""
    foreach ($candidate in @($env:HOME, $env:USERPROFILE)) {
        if ($candidate) {
            $profileDir = $candidate
            break
        }
    }
    $stateDir = ""
    $record = ""
    if ($profileDir) {
        $stateDir = Join-Path $profileDir ".bravebot"
        $record = Join-Path $stateDir "installed-by-powershell"
    }

    # Where the last install put it, so running this again updates that copy instead of leaving a second one
    # somewhere else on the PATH. INSTALL_DIR wins, for a person who is moving it. A record that does not
    # name a directory is no record.
    $installDir = $env:INSTALL_DIR
    if (-not $installDir -and $record -and (Test-Path -LiteralPath $record -PathType Leaf)) {
        try {
            $recorded = Get-Content -LiteralPath $record -Encoding UTF8 -TotalCount 1
            if ($recorded) {
                $installDir = [System.IO.Path]::GetDirectoryName($recorded)
            }
        } catch {
            $installDir = $null
        }
    }
    if (-not $installDir) {
        if (-not $env:LOCALAPPDATA) {
            Fail "nowhere to install: set INSTALL_DIR, or run this on Windows, where LOCALAPPDATA names the per-user program directory"
        }
        $installDir = Join-Path (Join-Path $env:LOCALAPPDATA "Programs") "bravebot"
    }
    $installDir = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($installDir)

    if (-not $tag) {
        try {
            $latest = [string](Invoke-RestMethod -Uri $ApiUrl).tag_name
        } catch {
            $latest = ""
        }
        # The tag is composed into a URL, so it is held to the shape of one that is published.
        if ($latest.StartsWith("v", [StringComparison]::Ordinal) -and (Test-Version $latest.Substring(1))) {
            $tag = $latest
        } else {
            Fail "unable to resolve the latest release tag from $ApiUrl"
        }
    }

    $baseUrl = "https://github.com/$Repo/releases/download/$tag"
    $temp = Join-Path ([System.IO.Path]::GetTempPath()) ("bravebot-install-" + [Guid]::NewGuid().ToString("N"))
    [System.IO.Directory]::CreateDirectory($temp) | Out-Null
    try {
        # Named for the platform, with the extension the signature check reads a file by.
        $binPath = Join-Path $temp $asset
        $shaPath = Join-Path $temp "$asset.sha256"

        Write-Host "Downloading $asset $tag..."
        try {
            Invoke-WebRequest -UseBasicParsing -Uri "$baseUrl/$asset" -OutFile $binPath
        } catch {
            Fail "unable to download $asset for $tag; check that this release exists for your platform"
        }
        try {
            Invoke-WebRequest -UseBasicParsing -Uri "$baseUrl/$asset.sha256" -OutFile $shaPath
        } catch {
            Fail "unable to download the checksum for $asset"
        }

        $expected = ([string](Get-Content -LiteralPath $shaPath -Raw)) -replace '\s', ''
        if (-not (Test-Sha256 $expected)) {
            Fail "malformed checksum for $asset"
        }
        # Compared as two digests and not as two spellings, so the case either was written in is no mismatch.
        $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $binPath).Hash
        if ($actual -ine $expected) {
            [Console]::Error.WriteLine("expected: $expected")
            [Console]::Error.WriteLine("actual:   $actual")
            Fail "checksum mismatch for $asset"
        }

        # The checksum shows the download arrived intact and not who made it, so this is the check that
        # does. Status has to be exactly Valid and the signing certificate's name exactly Brave's.
        $signature = Get-AuthenticodeSignature -LiteralPath $binPath
        $status = [string]$signature.Status
        $signer = ""
        if ($signature.SignerCertificate) {
            $signer = [string]$signature.SignerCertificate.GetNameInfo("SimpleName", $false)
        }
        if ($status -cne "Valid" -or $signer -cne $SignerName) {
            $reportedSigner = if ($signer) { $signer } else { "none" }
            Fail "$asset is not signed by $SignerName (Authenticode reports $status, signer $reportedSigner); refusing to install"
        }

        # Written beside its destination and then moved over it, so the binary a person runs is either the
        # old one or the whole new one. A copy that cannot be replaced, because bravebot is running from
        # it, leaves the old one where it was. The file calls are .NET's and not the cmdlets', which read
        # a `[` in a directory's name as the start of a pattern and not as a letter.
        $destination = Join-Path $installDir $BinName
        $staged = Join-Path $installDir (".$BinName." + [Guid]::NewGuid().ToString("N"))
        try {
            [System.IO.Directory]::CreateDirectory($installDir) | Out-Null
            [System.IO.File]::Copy($binPath, $staged)
            if ([System.IO.File]::Exists($destination)) {
                [System.IO.File]::Replace($staged, $destination, [NullString]::Value)
            } else {
                [System.IO.File]::Move($staged, $destination)
            }
        } catch {
            Fail "unable to write ${destination} ($($_.Exception.Message)); if bravebot is running from there, close it and run this again"
        } finally {
            Remove-Item -LiteralPath $staged -Force -ErrorAction SilentlyContinue
        }
    } finally {
        Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
    }

    # What bravebot reads to know it can offer the command that updates this copy. A machine with no
    # profile directory gets the binary and no notice about later releases, which is the same as no record
    # at all. The record is removed and written again rather than written over, so a link left where it goes
    # is replaced and not followed, and it is UTF-8 with no byte order mark, which a reader of the first
    # line would take for part of the path.
    if ($stateDir) {
        try {
            [System.IO.Directory]::CreateDirectory($stateDir) | Out-Null
            Remove-Item -LiteralPath $record -Force -ErrorAction SilentlyContinue
            [System.IO.File]::WriteAllText($record, "$destination`n", (New-Object System.Text.UTF8Encoding($false)))
        } catch {
            Write-Host "note: could not record where bravebot was installed, so bravebot cannot name the command that updates it."
        }
    }

    Write-Host "Installed bravebot $tag to $destination"

    $onPath = $false
    if ($env:PATH) {
        $wantedEntry = $installDir.TrimEnd("\", "/")
        foreach ($entry in $env:PATH.Split([System.IO.Path]::PathSeparator)) {
            try {
                $full = [System.IO.Path]::GetFullPath($entry).TrimEnd("\", "/")
            } catch {
                continue
            }
            if ($full -ieq $wantedEntry) {
                $onPath = $true
                break
            }
        }
    }
    if ($onPath) {
        Write-Host "Run: bravebot --help"
    } else {
        Write-Host "note: $installDir is not on your PATH; add it to your user PATH to run bravebot by name, then open a new terminal."
    }
} -Requested $Version -Given ($PSBoundParameters.ContainsKey("Version"))
