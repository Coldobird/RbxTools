param(
    [ValidateSet("Snapshot", "Monitor", "Experiment", "ValidateCandidates")]
    [string]$Mode = "Snapshot",
    [int]$DurationSeconds = 10,
    [int]$IntervalMilliseconds = 500,
    [int]$LongBlockSeconds = 1800,
    [int]$GraceSeconds = 3,
    [int]$CandidateObservationSeconds = 4,
    [string]$FixedCandidateOrder,
    [string]$CandidateStatsPath,
    [switch]$Simulation,
    [int]$SimulatedRecoveryAfterCandidates = 2,
    [string]$OutputPath,
    [switch]$Elevated
)

$ErrorActionPreference = "Stop"

function Find-SteamApiRuntime {
    $steamRoot = "C:\Program Files (x86)\Steam"
    $candidates = Get-ChildItem "$steamRoot\steamapps\common" -Filter steam_api64.dll -Recurse -ErrorAction SilentlyContinue
    if (-not $candidates) {
        throw "No installed steam_api64.dll runtime was found."
    }
    $candidates |
        Sort-Object @{ Expression = { $_.VersionInfo.FileVersionRaw }; Descending = $true }, LastWriteTime -Descending |
        Select-Object -First 1
}

function Get-IsElevated {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

$endpointHashes = @{}
function Get-EndpointHash([string]$Endpoint) {
    if (-not $endpointHashes.ContainsKey($Endpoint)) {
        $sha = [Security.Cryptography.SHA256]::Create()
        try {
            $bytes = [Text.Encoding]::UTF8.GetBytes($Endpoint)
            $hash = $sha.ComputeHash($bytes)
            $endpointHashes[$Endpoint] = ([BitConverter]::ToString($hash) -replace "-", "").Substring(0, 12)
        } finally { $sha.Dispose() }
    }
    $endpointHashes[$Endpoint]
}

function Get-SteamSocketSnapshot([int]$ProcessId) {
    $connections = [Collections.Generic.List[object]]::new()
    foreach ($line in (& netstat.exe -ano 2>$null)) {
        $parts = @($line.Trim() -split '\s+')
        if ($parts.Count -lt 4 -or $parts[-1] -ne [string]$ProcessId) { continue }
        if ($parts[0] -eq "TCP" -and $parts.Count -ge 5) {
            $remote = $parts[2]
            $lastColon = $remote.LastIndexOf(":")
            $remotePort = if ($lastColon -ge 0) { $remote.Substring($lastColon + 1) } else { "" }
            $connections.Add([pscustomobject]@{
                protocol = "tcp"
                state = $parts[3]
                remotePort = $remotePort
                addressFamily = if ($remote.StartsWith("[")) { "ipv6" } else { "ipv4" }
                endpointId = Get-EndpointHash $remote
            })
        } elseif ($parts[0] -eq "UDP") {
            $connections.Add([pscustomobject]@{
                protocol = "udp"
                state = "BOUND"
                remotePort = ""
                addressFamily = if ($parts[1].StartsWith("[")) { "ipv6" } else { "ipv4" }
                endpointId = Get-EndpointHash $parts[1]
            })
        }
    }
    @($connections)
}

function Read-ReconnectLogEvents([string]$Path, [long]$StartOffset) {
    if (-not (Test-Path -LiteralPath $Path)) { return @() }
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
    try {
        if ($stream.Length -lt $StartOffset) { $StartOffset = 0 }
        $stream.Seek($StartOffset, [IO.SeekOrigin]::Begin) | Out-Null
        $reader = [IO.StreamReader]::new($stream)
        try { $text = $reader.ReadToEnd() } finally { $reader.Dispose() }
    } finally { $stream.Dispose() }
    @(
        $text -split "`r?`n" |
            Where-Object { $_ -match 'ScheduleImmediateReconnect|StartAutoReconnect|ScheduledAttemptReconnect|connected|disconnected|logon|logged on|connectivity test' } |
            ForEach-Object {
                $sanitized = $_ -replace '\b\d{17}\b', '<steam-id>'
                $sanitized = $sanitized -replace '\[U:\d+:\d+\]', '[steam-user]'
                $sanitized = $sanitized -replace '\[[0-9A-Fa-f:]+\](?::\d+)?', '<ipv6>'
                $sanitized = $sanitized -replace '\b(?:[0-9A-Fa-f]{0,4}:){3,}[0-9A-Fa-f:]{1,4}\b', '<ipv6>'
                $sanitized = $sanitized -replace '(?<![\w:])(?:\d{1,3}\.){3}\d{1,3}(?::\d+)?', '<ipv4>'
                [pscustomobject]@{ text = $sanitized }
            }
    )
}

function Read-CandidateStats([string]$Path, [string[]]$Names) {
    $stats = @{}
    if (Test-Path -LiteralPath $Path) {
        $saved = Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json
        foreach ($property in $saved.PSObject.Properties) {
            $stats[$property.Name] = @{
                attempts = [int]$property.Value.attempts
                recovered = [int]$property.Value.recovered
                errors = [int]$property.Value.errors
            }
        }
    }
    foreach ($name in $Names) {
        if (-not $stats.ContainsKey($name)) {
            $stats[$name] = @{ attempts = 0; recovered = 0; errors = 0 }
        }
    }
    $stats
}

function Save-CandidateStats([string]$Path, [hashtable]$Stats) {
    $ordered = [ordered]@{}
    foreach ($name in @($Stats.Keys | Sort-Object)) {
        $ordered[$name] = $Stats[$name]
    }
    $ordered | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 -LiteralPath $Path
}

function Get-CandidateOrder([string[]]$Names, [hashtable]$Stats) {
    @(
        $Names |
            ForEach-Object {
                [pscustomobject]@{ name = $_; attempts = $Stats[$_].attempts; tieBreaker = Get-Random }
            } |
            Sort-Object attempts, tieBreaker |
            ForEach-Object name
    )
}

if (-not ("RbxSteamProbe.Native" -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

namespace RbxSteamProbe {
    public sealed class BlockDetector : IDisposable {
        [DllImport("fwpuclnt.dll", EntryPoint = "FwpmEngineOpen0")]
        private static extern uint EngineOpen(IntPtr serverName, uint authnService, IntPtr authIdentity, IntPtr session, out IntPtr engine);
        [DllImport("fwpuclnt.dll", EntryPoint = "FwpmEngineClose0")]
        private static extern uint EngineClose(IntPtr engine);
        [DllImport("fwpuclnt.dll", EntryPoint = "FwpmSubLayerGetByKey0")]
        private static extern uint GetSubLayer(IntPtr engine, ref Guid key, out IntPtr subLayer);
        [DllImport("fwpuclnt.dll", EntryPoint = "FwpmFreeMemory0")]
        private static extern void FreeMemory(ref IntPtr memory);

        private IntPtr engine;
        private readonly Guid rbxSubLayer = new Guid("68b17ba8-f37e-4d7e-937b-c2727885438b");

        public BlockDetector() {
            uint result = EngineOpen(IntPtr.Zero, 10, IntPtr.Zero, IntPtr.Zero, out engine);
            if (result != 0) throw new InvalidOperationException("Could not open WFP engine: 0x" + result.ToString("X8"));
        }

        public bool IsBlocked {
            get {
                IntPtr found;
                Guid key = rbxSubLayer;
                uint result = GetSubLayer(engine, ref key, out found);
                if (result == 0 && found != IntPtr.Zero) FreeMemory(ref found);
                return result == 0;
            }
        }

        public void Dispose() {
            if (engine != IntPtr.Zero) {
                EngineClose(engine);
                engine = IntPtr.Zero;
            }
        }
    }

    public sealed class Native : IDisposable {
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern IntPtr LoadLibraryW(string path);
        [DllImport("kernel32.dll", CharSet = CharSet.Ansi, SetLastError = true)]
        private static extern IntPtr GetProcAddress(IntPtr module, string name);
        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool FreeLibrary(IntPtr module);

        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate int InitFlatDelegate(IntPtr errorMessage);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate void ShutdownDelegate();
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate IntPtr SteamUserDelegate();
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.I1)]
        private delegate bool BLoggedOnDelegate(IntPtr self);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate UInt64 GetSteamIdDelegate(IntPtr self);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate UInt64 RequestUserStatsDelegate(IntPtr self, UInt64 steamId);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate IntPtr InterfaceDelegate();
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate UInt64 EnumerateSubscribedDelegate(IntPtr self, UInt32 startIndex);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate void InitRelayDelegate(IntPtr self);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate int InitAuthenticationDelegate(IntPtr self);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.I1)]
        private delegate bool RequestUserInformationDelegate(IntPtr self, UInt64 steamId, [MarshalAs(UnmanagedType.I1)] bool requireNameOnly);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate UInt64 RequestLobbyListDelegate(IntPtr self);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate UInt64 CreateUserUgcQueryDelegate(IntPtr self, UInt32 accountId, int listType, int matchingType, int sortOrder, UInt32 creatorAppId, UInt32 consumerAppId, UInt32 page);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate UInt64 SendUgcQueryDelegate(IntPtr self, UInt64 query);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.I1)]
        private delegate bool ReleaseUgcQueryDelegate(IntPtr self, UInt64 query);
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate void RunCallbacksDelegate();

        private readonly IntPtr module;
        private readonly InitFlatDelegate initFlat;
        private readonly ShutdownDelegate shutdown;
        private readonly SteamUserDelegate steamUser;
        private readonly BLoggedOnDelegate bLoggedOn;
        private readonly GetSteamIdDelegate getSteamId;
        private readonly RequestUserStatsDelegate requestUserStats;
        private readonly InterfaceDelegate steamUserStats;
        private readonly InterfaceDelegate steamRemoteStorage;
        private readonly InterfaceDelegate steamNetworkingUtils;
        private readonly InterfaceDelegate steamNetworkingSockets;
        private readonly InterfaceDelegate steamFriends;
        private readonly InterfaceDelegate steamMatchmaking;
        private readonly InterfaceDelegate steamUgc;
        private readonly EnumerateSubscribedDelegate enumerateSubscribed;
        private readonly InitRelayDelegate initRelay;
        private readonly InitAuthenticationDelegate initAuthentication;
        private readonly RequestUserInformationDelegate requestUserInformation;
        private readonly RequestLobbyListDelegate requestLobbyList;
        private readonly CreateUserUgcQueryDelegate createUserUgcQuery;
        private readonly SendUgcQueryDelegate sendUgcQuery;
        private readonly ReleaseUgcQueryDelegate releaseUgcQuery;
        private readonly RunCallbacksDelegate runCallbacks;
        private readonly System.Collections.Generic.List<UInt64> ugcQueries = new System.Collections.Generic.List<UInt64>();
        private bool initialized;

        public Native(string dllPath) {
            module = LoadLibraryW(dllPath);
            if (module == IntPtr.Zero) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
            initFlat = Bind<InitFlatDelegate>("SteamAPI_InitFlat");
            shutdown = Bind<ShutdownDelegate>("SteamAPI_Shutdown");
            steamUser = Bind<SteamUserDelegate>("SteamAPI_SteamUser_v023");
            bLoggedOn = Bind<BLoggedOnDelegate>("SteamAPI_ISteamUser_BLoggedOn");
            getSteamId = Bind<GetSteamIdDelegate>("SteamAPI_ISteamUser_GetSteamID");
            steamUserStats = Bind<InterfaceDelegate>("SteamAPI_SteamUserStats_v013");
            requestUserStats = Bind<RequestUserStatsDelegate>("SteamAPI_ISteamUserStats_RequestUserStats");
            steamRemoteStorage = Bind<InterfaceDelegate>("SteamAPI_SteamRemoteStorage_v016");
            enumerateSubscribed = Bind<EnumerateSubscribedDelegate>("SteamAPI_ISteamRemoteStorage_EnumerateUserSubscribedFiles");
            steamNetworkingUtils = Bind<InterfaceDelegate>("SteamAPI_SteamNetworkingUtils_SteamAPI_v004");
            initRelay = Bind<InitRelayDelegate>("SteamAPI_ISteamNetworkingUtils_InitRelayNetworkAccess");
            steamNetworkingSockets = Bind<InterfaceDelegate>("SteamAPI_SteamNetworkingSockets_SteamAPI_v012");
            initAuthentication = Bind<InitAuthenticationDelegate>("SteamAPI_ISteamNetworkingSockets_InitAuthentication");
            steamFriends = Bind<InterfaceDelegate>("SteamAPI_SteamFriends_v018");
            requestUserInformation = Bind<RequestUserInformationDelegate>("SteamAPI_ISteamFriends_RequestUserInformation");
            steamMatchmaking = Bind<InterfaceDelegate>("SteamAPI_SteamMatchmaking_v009");
            requestLobbyList = Bind<RequestLobbyListDelegate>("SteamAPI_ISteamMatchmaking_RequestLobbyList");
            steamUgc = Bind<InterfaceDelegate>("SteamAPI_SteamUGC_v021");
            createUserUgcQuery = Bind<CreateUserUgcQueryDelegate>("SteamAPI_ISteamUGC_CreateQueryUserUGCRequest");
            sendUgcQuery = Bind<SendUgcQueryDelegate>("SteamAPI_ISteamUGC_SendQueryUGCRequest");
            releaseUgcQuery = Bind<ReleaseUgcQueryDelegate>("SteamAPI_ISteamUGC_ReleaseQueryUGCRequest");
            runCallbacks = Bind<RunCallbacksDelegate>("SteamAPI_RunCallbacks");
        }

        private T Bind<T>(string name) where T : class {
            IntPtr address = GetProcAddress(module, name);
            if (address == IntPtr.Zero) throw new EntryPointNotFoundException(name);
            return Marshal.GetDelegateForFunctionPointer(address, typeof(T)) as T;
        }

        public int Initialize(out string error) {
            IntPtr buffer = Marshal.AllocHGlobal(1024);
            try {
                for (int i = 0; i < 1024; i++) Marshal.WriteByte(buffer, i, 0);
                int result = initFlat(buffer);
                error = Marshal.PtrToStringAnsi(buffer) ?? "";
                initialized = result == 0;
                return result;
            } finally {
                Marshal.FreeHGlobal(buffer);
            }
        }

        public bool HasUserInterface { get { return steamUser() != IntPtr.Zero; } }
        public bool IsLoggedOn {
            get {
                IntPtr user = steamUser();
                return user != IntPtr.Zero && bLoggedOn(user);
            }
        }

        public void RunCallbacks() { runCallbacks(); }

        public string InvokeCandidate(string name) {
            IntPtr user = steamUser();
            if (user == IntPtr.Zero) throw new InvalidOperationException("Steam user interface unavailable.");
            UInt64 steamId = getSteamId(user);
            switch (name) {
                case "request-user-stats":
                    return "call=" + requestUserStats(steamUserStats(), steamId);
                case "enumerate-subscribed-files":
                    return "call=" + enumerateSubscribed(steamRemoteStorage(), 0);
                case "init-relay-network-access":
                    initRelay(steamNetworkingUtils());
                    return "requested";
                case "init-network-authentication":
                    return "availability=" + initAuthentication(steamNetworkingSockets());
                case "request-user-information":
                    return "requested=" + requestUserInformation(steamFriends(), steamId, true);
                case "request-lobby-list":
                    return "call=" + requestLobbyList(steamMatchmaking());
                case "query-subscribed-ugc":
                    UInt64 query = createUserUgcQuery(steamUgc(), (UInt32)steamId, 6, 0, 0, 480, 480, 1);
                    if (query == UInt64.MaxValue) return "invalid-query";
                    ugcQueries.Add(query);
                    return "query=" + query + ";call=" + sendUgcQuery(steamUgc(), query);
                default:
                    throw new ArgumentException("Unknown candidate: " + name);
            }
        }

        public void Dispose() {
            if (initialized) {
                IntPtr ugc = steamUgc();
                foreach (UInt64 query in ugcQueries) releaseUgcQuery(ugc, query);
                ugcQueries.Clear();
                shutdown();
                initialized = false;
            }
            if (module != IntPtr.Zero) FreeLibrary(module);
        }
    }
}
'@
}

$runtime = Find-SteamApiRuntime
$steam = Get-Process steam -ErrorAction Stop | Select-Object -First 1
$isElevated = Get-IsElevated
$diagnosticDir = Join-Path $env:LOCALAPPDATA "RBX Tools\diagnostics"
if (-not $OutputPath) {
    New-Item -ItemType Directory -Force -Path $diagnosticDir | Out-Null
    $privilegeLabel = if ($Elevated -or $isElevated) { "elevated" } else { "desktop" }
    $OutputPath = Join-Path $diagnosticDir ("steam-probe-{0}-{1}.json" -f (Get-Date -Format "yyyyMMdd-HHmmss"), $privilegeLabel)
}

if ($Elevated -and -not $isElevated) {
    $arguments = @(
        "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", ('"' + $PSCommandPath + '"'),
        "-Mode", $Mode, "-DurationSeconds", $DurationSeconds,
        "-IntervalMilliseconds", $IntervalMilliseconds,
        "-LongBlockSeconds", $LongBlockSeconds, "-GraceSeconds", $GraceSeconds,
        "-CandidateObservationSeconds", $CandidateObservationSeconds,
        "-OutputPath", ('"' + $OutputPath + '"')
    )
    if ($CandidateStatsPath) { $arguments += @("-CandidateStatsPath", ('"' + $CandidateStatsPath + '"')) }
    if ($FixedCandidateOrder) { $arguments += @("-FixedCandidateOrder", ('"' + $FixedCandidateOrder + '"')) }
    if ($Simulation) { $arguments += @("-Simulation", "-SimulatedRecoveryAfterCandidates", $SimulatedRecoveryAfterCandidates) }
    $process = Start-Process powershell.exe -Verb RunAs -WindowStyle Hidden -ArgumentList $arguments -PassThru -Wait
    if ($process.ExitCode -ne 0) { throw "Elevated probe exited with code $($process.ExitCode)." }
    Get-Content -Raw $OutputPath | ConvertFrom-Json
    exit 0
}

$previousAppId = $env:SteamAppId
$previousGameId = $env:SteamGameId
$env:SteamAppId = "480"
$env:SteamGameId = "480"
$startedAt = [DateTimeOffset]::UtcNow
$samples = [Collections.Generic.List[object]]::new()
$native = $null
$blockDetector = $null
$connectionLog = "C:\Program Files (x86)\Steam\logs\connection_log.txt"
$connectionLogOffset = if (Test-Path -LiteralPath $connectionLog) { (Get-Item -LiteralPath $connectionLog).Length } else { 0 }
$allCandidateNames = @(
    "request-user-stats",
    "enumerate-subscribed-files",
    "query-subscribed-ugc",
    "init-relay-network-access",
    "init-network-authentication",
    "request-user-information",
    "request-lobby-list"
)
$useFixedCandidateOrder = -not [string]::IsNullOrWhiteSpace($FixedCandidateOrder)
$candidateNames = if ($useFixedCandidateOrder) {
    @($FixedCandidateOrder.Split(',') | ForEach-Object { $_.Trim() } | Where-Object { $_ })
} else {
    @($allCandidateNames)
}
foreach ($candidateName in $candidateNames) {
    if ($candidateName -notin $allCandidateNames) {
        throw "Unknown candidate in fixed order: $candidateName"
    }
}
if ($candidateNames.Count -eq 0) { throw "Candidate order cannot be empty." }
$candidateStatsPath = if ($CandidateStatsPath) { $CandidateStatsPath } else { Join-Path $diagnosticDir "steam-candidate-stats.json" }
$candidateStats = Read-CandidateStats $candidateStatsPath $candidateNames
$candidateAttempts = [Collections.Generic.List[object]]::new()
$experimentEvents = [Collections.Generic.List[object]]::new()
$compatibility = [Collections.Generic.List[object]]::new()

try {
    $native = [RbxSteamProbe.Native]::new($runtime.FullName)
    $initError = ""
    $initResult = $native.Initialize([ref]$initError)
    $hasUserInterface = $false
    if ($initResult -eq 0) { $hasUserInterface = $native.HasUserInterface }
    if ($Mode -eq "Experiment") { $blockDetector = [RbxSteamProbe.BlockDetector]::new() }

    if ($Mode -eq "ValidateCandidates" -and $initResult -eq 0 -and $hasUserInterface) {
        foreach ($candidate in $candidateNames) {
            $attemptStarted = [DateTimeOffset]::UtcNow
            try {
                $detail = $native.InvokeCandidate($candidate)
                for ($callbackIndex = 0; $callbackIndex -lt 4; $callbackIndex++) {
                    $native.RunCallbacks()
                    Start-Sleep -Milliseconds 250
                }
                $compatibility.Add([pscustomobject]@{ name = $candidate; supported = $true; detail = $detail; error = $null; startedUtc = $attemptStarted.ToString("O") })
            } catch {
                $compatibility.Add([pscustomobject]@{ name = $candidate; supported = $false; detail = $null; error = $_.Exception.Message; startedUtc = $attemptStarted.ToString("O") })
            }
        }
    }

    $stopwatch = [Diagnostics.Stopwatch]::StartNew()
    $previousBlocked = if ($blockDetector) { $blockDetector.IsBlocked } else { $false }
    $blockedSinceMs = if ($previousBlocked) { -1L } else { $null }
    $graceDeadlineMs = $null
    $candidateQueue = @()
    $candidateIndex = 0
    $activeCandidate = $null
    $candidateDeadlineMs = $null
    $simulationSawBlock = $false
    do {
        if ($initResult -eq 0) { $native.RunCallbacks() }
        $loggedOn = $false
        if ($initResult -eq 0 -and $hasUserInterface) { $loggedOn = $native.IsLoggedOn }
        $nowMs = $stopwatch.ElapsedMilliseconds
        $isBlocked = if ($blockDetector) { $blockDetector.IsBlocked } else { $false }
        if ($Simulation -and $isBlocked) { $simulationSawBlock = $true }
        if ($Simulation -and $simulationSawBlock -and ($isBlocked -or $candidateAttempts.Count -lt $SimulatedRecoveryAfterCandidates)) {
            $loggedOn = $false
        }

        if ($blockDetector -and $isBlocked -ne $previousBlocked) {
            if ($isBlocked) {
                if ($activeCandidate -or $candidateQueue.Count -gt $candidateIndex -or $null -ne $graceDeadlineMs) {
                    $experimentEvents.Add([pscustomobject]@{ elapsedMilliseconds = $nowMs; type = "candidate-sequence-cancelled"; detail = "network-blocked-again" })
                }
                $blockedSinceMs = $nowMs
                $graceDeadlineMs = $null
                $candidateQueue = @()
                $candidateIndex = 0
                $activeCandidate = $null
                $candidateDeadlineMs = $null
                $experimentEvents.Add([pscustomobject]@{ elapsedMilliseconds = $nowMs; type = "block-started"; detail = $null })
            } else {
                $blockDurationMs = if ($null -eq $blockedSinceMs -or $blockedSinceMs -lt 0) { $null } else { $nowMs - $blockedSinceMs }
                $experimentEvents.Add([pscustomobject]@{ elapsedMilliseconds = $nowMs; type = "network-restored"; detail = "blockedMilliseconds=$blockDurationMs" })
                if ($null -ne $blockDurationMs -and $blockDurationMs -ge ($LongBlockSeconds * 1000)) {
                    $graceDeadlineMs = $nowMs + ($GraceSeconds * 1000)
                }
                $blockedSinceMs = $null
            }
            $previousBlocked = $isBlocked
        }

        if (-not $isBlocked -and $null -ne $graceDeadlineMs -and $nowMs -ge $graceDeadlineMs) {
            if ($loggedOn) {
                $experimentEvents.Add([pscustomobject]@{ elapsedMilliseconds = $nowMs; type = "recovered-during-grace"; detail = $null })
            } else {
                $candidateQueue = if ($useFixedCandidateOrder) {
                    @($candidateNames)
                } else {
                    @(Get-CandidateOrder $candidateNames $candidateStats)
                }
                $experimentEvents.Add([pscustomobject]@{ elapsedMilliseconds = $nowMs; type = "candidate-sequence-started"; detail = ($candidateQueue -join ",") })
            }
            $graceDeadlineMs = $null
        }

        if ($activeCandidate -and $loggedOn) {
            $candidateStats[$activeCandidate].recovered++
            Save-CandidateStats $candidateStatsPath $candidateStats
            $experimentEvents.Add([pscustomobject]@{ elapsedMilliseconds = $nowMs; type = "recovered-after-candidate"; detail = $activeCandidate })
            $activeCandidate = $null
            $candidateQueue = @()
            $candidateDeadlineMs = $null
        } elseif ($activeCandidate -and $nowMs -ge $candidateDeadlineMs) {
            $activeCandidate = $null
            $candidateDeadlineMs = $null
        }

        if (-not $isBlocked -and -not $loggedOn -and -not $activeCandidate -and $candidateIndex -lt $candidateQueue.Count) {
            $activeCandidate = $candidateQueue[$candidateIndex++]
            $candidateStats[$activeCandidate].attempts++
            $attemptError = $null
            $attemptDetail = $null
            try {
                $attemptDetail = if ($Simulation) { "simulated" } else { $native.InvokeCandidate($activeCandidate) }
            }
            catch {
                $attemptError = $_.Exception.Message
                $candidateStats[$activeCandidate].errors++
            }
            Save-CandidateStats $candidateStatsPath $candidateStats
            $candidateAttempts.Add([pscustomobject]@{
                elapsedMilliseconds = $nowMs
                name = $activeCandidate
                detail = $attemptDetail
                error = $attemptError
            })
            $candidateDeadlineMs = $nowMs + ($CandidateObservationSeconds * 1000)
        }

        $connections = Get-SteamSocketSnapshot $steam.Id
        $samples.Add([pscustomobject]@{
            elapsedMilliseconds = $nowMs
            utc = [DateTimeOffset]::UtcNow.ToString("O")
            loggedOn = $loggedOn
            blocked = $isBlocked
            activeCandidate = $activeCandidate
            connections = $connections
        })
        if ($Mode -eq "Snapshot") { break }
        Start-Sleep -Milliseconds ([Math]::Max(100, $IntervalMilliseconds))
    } while ($stopwatch.Elapsed.TotalSeconds -lt $DurationSeconds)

    $transitionCount = 0
    $firstLoggedOffMs = $null
    $firstRecoveredMs = $null
    for ($index = 0; $index -lt $samples.Count; $index++) {
        if (-not $samples[$index].loggedOn -and $null -eq $firstLoggedOffMs) {
            $firstLoggedOffMs = $samples[$index].elapsedMilliseconds
        }
        if ($index -gt 0 -and $samples[$index].loggedOn -ne $samples[$index - 1].loggedOn) {
            $transitionCount++
        }
        if ($null -ne $firstLoggedOffMs -and $samples[$index].loggedOn -and $null -eq $firstRecoveredMs) {
            $firstRecoveredMs = $samples[$index].elapsedMilliseconds
        }
    }
    $uniqueEndpoints = @($samples.connections.endpointId | Sort-Object -Unique).Count

    $result = [pscustomobject]@{
        schemaVersion = 1
        startedUtc = $startedAt.ToString("O")
        finishedUtc = [DateTimeOffset]::UtcNow.ToString("O")
        elevated = $isElevated
        mode = $Mode
        steamProcessId = $steam.Id
        steamProcessStartedUtc = $steam.StartTime.ToUniversalTime().ToString("O")
        steamApiRuntimePath = $runtime.FullName
        steamApiRuntimeVersion = $runtime.VersionInfo.FileVersion
        appId = 480
        initResult = $initResult
        initError = $initError
        userInterfacePresent = $hasUserInterface
        experimentConfiguration = [pscustomobject]@{
            longBlockSeconds = $LongBlockSeconds
            graceSeconds = $GraceSeconds
            candidateObservationSeconds = $CandidateObservationSeconds
            fixedCandidateOrder = if ($useFixedCandidateOrder) { @($candidateNames) } else { $null }
        }
        summary = [pscustomobject]@{
            sampleCount = $samples.Count
            transitionCount = $transitionCount
            firstLoggedOffMilliseconds = $firstLoggedOffMs
            firstRecoveredMilliseconds = $firstRecoveredMs
            recoveryDurationMilliseconds = if ($null -ne $firstRecoveredMs) { $firstRecoveredMs - $firstLoggedOffMs } else { $null }
            uniqueSocketEndpoints = $uniqueEndpoints
        }
        samples = $samples
        compatibility = $compatibility
        experimentEvents = $experimentEvents
        candidateAttempts = $candidateAttempts
        candidateStatsPath = $candidateStatsPath
        reconnectLogEvents = @(Read-ReconnectLogEvents $connectionLog $connectionLogOffset)
    }
    if ($OutputPath) {
        $parent = Split-Path -Parent $OutputPath
        if ($parent) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
        $result | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 $OutputPath
    }
    $result
} finally {
    if ($native) { $native.Dispose() }
    if ($blockDetector) { $blockDetector.Dispose() }
    $env:SteamAppId = $previousAppId
    $env:SteamGameId = $previousGameId
}
