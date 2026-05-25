#requires -RunAsAdministrator
<#
.SYNOPSIS
    Windows transparent proxy launcher with domain-based loop-prevention routes.

.DESCRIPTION
    This script sets up a full transparent proxy stack on Windows:

      1. Detects the physical network interface and its default gateway.
      2. Resolves the proxy server domain and pins its current IPs to the physical NIC.
      3. Starts the SOCKS5 client.
      4. Starts tun2socks on a Wintun-based TUN interface.
      5. Configures the TUN interface and routes all system traffic through it.
      6. Refreshes domain-based bypass routes on a timer to follow DNS changes.

    The key idea is to keep the proxy server domain/IPs on the physical NIC,
    so the client does not loop back into the TUN path.

.USAGE
    PowerShell (run as Administrator):
      .\proxy.ps1 -ServerHost proxy.example.com -ClientArgs @('--server','proxy.example.com','--token','secret')

    Custom binaries:
      .\proxy.ps1 -ClientPath .\client.exe -Tun2SocksPath .\tun2socks.exe `
                  -ServerHost proxy.example.com `
                  -ClientArgs @('--server','proxy.example.com','--token','secret')

.NOTES
    - This script assumes your tun2socks build accepts flags similar to:
        -device <interfaceName> -proxy socks5://127.0.0.1:<port>
      If your build uses different flags, adjust the Tun2SocksArgs section.
    - Domain-based bypass is implemented as host routes for the IPs currently
      returned by DNS. If the domain uses rapidly changing CDN IPs, reduce the
      refresh interval.
#>

param(
    [string]$ClientPath = (Join-Path $PSScriptRoot 'client.exe'),
    [string]$Tun2SocksPath = (Join-Path $PSScriptRoot 'tun2socks.exe'),
    [string]$InterfaceName = 'tun0',
    [string]$SocksHost = '127.0.0.1',
    [int]$SocksPort = 1080,
    [string]$ServerHost = 'proxy.example.com',
    [string]$DnsServer = '',
    [int]$RefreshIntervalSec = 300,
    [int]$TunnelIPv4PrefixLength = 16,
    [string]$TunnelIPv4Address = '198.18.0.1',
    [string]$TunnelIPv6Address = 'fd00::1',
    [int]$TunnelIPv6PrefixLength = 64,
    [string[]]$ClientArgs = @()
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# ── Console colors ────────────────────────────────────────────────────────────
$Host.UI.RawUI.WindowTitle = 'Windows Transparent Proxy Launcher'
$Color = @{
    Red    = 'Red'
    Green  = 'Green'
    Yellow = 'Yellow'
    Cyan   = 'Cyan'
    White  = 'White'
}

function Write-Log  { param([string]$Message) Write-Host "[proxy] $Message" -ForegroundColor $Color.Cyan }
function Write-Ok   { param([string]$Message) Write-Host "[proxy] $Message" -ForegroundColor $Color.Green }
function Write-Warn { param([string]$Message) Write-Host "[proxy] $Message" -ForegroundColor $Color.Yellow }
function Write-Err  { param([string]$Message) Write-Host "[proxy] ERROR: $Message" -ForegroundColor $Color.Red }

function Fail {
    param([string]$Message)
    Write-Err $Message
    throw $Message
}

function Test-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

if (-not (Test-Administrator)) {
    Fail 'This script must be run from an elevated PowerShell session.'
}

# ── Runtime state ─────────────────────────────────────────────────────────────
$script:ClientProcess = $null
$script:Tun2SocksProcess = $null
$script:PhysicalIfIndex = $null
$script:PhysicalGateway4 = $null
$script:PhysicalIfAlias = $null
$script:PhysicalGateway6 = $null
$script:PhysicalIfIndex6 = $null
$script:BypassIPv4 = @()`n$script:BypassIPv6 = @()
$script:TunConfigured = $false
$script:RoutesConfigured = $false
$script:TunnelIfIndex = $null

function Get-DefaultRoutes {
    param(
        [ValidateSet('IPv4', 'IPv6')]
        [string]$AddressFamily
    )

    $prefix = if ($AddressFamily -eq 'IPv4') { '0.0.0.0/0' } else { '::/0' }
    Get-NetRoute -AddressFamily $AddressFamily -DestinationPrefix $prefix -ErrorAction SilentlyContinue |
        Sort-Object RouteMetric, InterfaceMetric, ifIndex
}

function Is-TunnelLikeInterface {
    param([string]$Alias, [string]$Description)

    $text = "$Alias $Description"
    return ($text -match '(?i)\b(tun|wintun|tap|wireguard|vpn)\b')
}

function Get-PhysicalGatewayInfo {
    param(
        [ValidateSet('IPv4', 'IPv6')]
        [string]$AddressFamily
    )

    $routes = Get-DefaultRoutes -AddressFamily $AddressFamily
    if (-not $routes) {
        return $null
    }

    foreach ($route in $routes) {
        $adapter = Get-NetAdapter -InterfaceIndex $route.ifIndex -ErrorAction SilentlyContinue
        if (-not $adapter) { continue }

        if (Is-TunnelLikeInterface -Alias $adapter.Name -Description $adapter.InterfaceDescription) {
            continue
        }

        if ($AddressFamily -eq 'IPv4' -and $route.NextHop -and $route.NextHop -ne '0.0.0.0') {
            return [pscustomobject]@{
                IfIndex  = $route.ifIndex
                Gateway  = $route.NextHop
                Alias    = $adapter.Name
                Family   = 'IPv4'
            }
        }

        if ($AddressFamily -eq 'IPv6' -and $route.NextHop -and $route.NextHop -ne '::') {
            return [pscustomobject]@{
                IfIndex  = $route.ifIndex
                Gateway  = $route.NextHop
                Alias    = $adapter.Name
                Family   = 'IPv6'
            }
        }
    }

    return $null
}

function Get-IpPrefixes {
    param([string[]]$IPs)

    foreach ($ip in $IPs) {
        if ([string]::IsNullOrWhiteSpace($ip)) { continue }
        if ($ip -match ':') { "$ip/128" } else { "$ip/32" }
    }
}

function Resolve-HostIPs {
    param([string]$HostName)

    $ips = New-Object System.Collections.Generic.List[string]

    try {
        if ([string]::IsNullOrWhiteSpace($DnsServer)) {
            $records = Resolve-DnsName -Name $HostName -Type A -ErrorAction SilentlyContinue
            foreach ($r in $records) {
                if ($r.IPAddress) { [void]$ips.Add($r.IPAddress) }
            }

            $records = Resolve-DnsName -Name $HostName -Type AAAA -ErrorAction SilentlyContinue
            foreach ($r in $records) {
                if ($r.IPAddress) { [void]$ips.Add($r.IPAddress) }
            }
        }
        else {
            $records = Resolve-DnsName -Name $HostName -Type A -Server $DnsServer -ErrorAction SilentlyContinue
            foreach ($r in $records) {
                if ($r.IPAddress) { [void]$ips.Add($r.IPAddress) }
            }

            $records = Resolve-DnsName -Name $HostName -Type AAAA -Server $DnsServer -ErrorAction SilentlyContinue
            foreach ($r in $records) {
                if ($r.IPAddress) { [void]$ips.Add($r.IPAddress) }
            }
        }
    }
    catch {
        try {
            $resolved = [System.Net.Dns]::GetHostAddresses($HostName)
            foreach ($addr in $resolved) {
                if ($addr.AddressFamily -eq 'InterNetwork' -or $addr.AddressFamily -eq 'InterNetworkV6') {
                    [void]$ips.Add($addr.IPAddressToString)
                }
            }
        }
        catch {
            Write-Warn "DNS resolution failed for $HostName"
        }
    }

    return $ips | Sort-Object -Unique
}

function Resolve-HostIPv4 {
    param([string]$HostName)
    @(Resolve-HostIPs -HostName $HostName | Where-Object { $_ -notmatch ':' })
}

function Resolve-HostIPv6 {
    param([string]$HostName)
    @(Resolve-HostIPs -HostName $HostName | Where-Object { $_ -match ':' })
}

function Add-BypassRoute {
    param(
        [Parameter(Mandatory=$true)][string]$IP,
        [Parameter(Mandatory=$true)][int]$IfIndex,
        [Parameter(Mandatory=$true)][string]$NextHop
    )

    $prefix = if ($IP -match ':') { "$IP/128" } else { "$IP/32" }

    Remove-NetRoute -DestinationPrefix $prefix -Confirm:$false -ErrorAction SilentlyContinue | Out-Null
    New-NetRoute -InterfaceIndex $IfIndex -DestinationPrefix $prefix -NextHop $NextHop -RouteMetric 1 -ErrorAction SilentlyContinue | Out-Null
}

function Remove-BypassRoute {
    param([Parameter(Mandatory=$true)][string]$IP)

    $prefix = if ($IP -match ':') { "$IP/128" } else { "$IP/32" }
    Remove-NetRoute -DestinationPrefix $prefix -Confirm:$false -ErrorAction SilentlyContinue | Out-Null
}

function Sync-BypassRoutesV4 {
    param(
        [Parameter(Mandatory=$true)][string]$HostName,
        [Parameter(Mandatory=$true)][int]$IfIndex,
        [Parameter(Mandatory=$true)][string]$NextHop
    )

    $resolved = @(Resolve-HostIPv4 -HostName $HostName)

    if (-not $resolved -or $resolved.Count -eq 0) {
        Write-Warn "No IPv4 addresses resolved for $HostName; keeping existing IPv4 bypass routes."
        return
    }

    $toAdd = $resolved | Where-Object { $_ -notin $script:BypassIPv4 }
    $toRemove = $script:BypassIPv4 | Where-Object { $_ -notin $resolved }

    foreach ($ip in $toAdd) {
        try {
            Add-BypassRoute -IP $ip -IfIndex $IfIndex -NextHop $NextHop
            Write-Ok "Pinned $HostName => $ip to the physical NIC"
        }
        catch {
            Write-Warn "Failed to add IPv4 bypass route for $ip"
        }
    }

    foreach ($ip in $toRemove) {
        try {
            Remove-BypassRoute -IP $ip
            Write-Log "Removed stale IPv4 bypass route for $ip"
        }
        catch {
            Write-Warn "Failed to remove stale IPv4 bypass route for $ip"
        }
    }

    $script:BypassIPv4 = $resolved
}

function Sync-BypassRoutesV6 {
    param(
        [Parameter(Mandatory=$true)][string]$HostName,
        [Parameter(Mandatory=$true)][int]$IfIndex,
        [Parameter(Mandatory=$true)][string]$NextHop
    )

    $resolved = @(Resolve-HostIPv6 -HostName $HostName)

    if (-not $resolved -or $resolved.Count -eq 0) {
        return
    }

    $toAdd = $resolved | Where-Object { $_ -notin $script:BypassIPv6 }
    $toRemove = $script:BypassIPv6 | Where-Object { $_ -notin $resolved }

    foreach ($ip in $toAdd) {
        try {
            Add-BypassRoute -IP $ip -IfIndex $IfIndex -NextHop $NextHop
            Write-Ok "Pinned $HostName => $ip to the physical NIC"
        }
        catch {
            Write-Warn "Failed to add IPv6 bypass route for $ip"
        }
    }

    foreach ($ip in $toRemove) {
        try {
            Remove-BypassRoute -IP $ip
            Write-Log "Removed stale IPv6 bypass route for $ip"
        }
        catch {
            Write-Warn "Failed to remove stale IPv6 bypass route for $ip"
        }
    }

    $script:BypassIPv6 = $resolved
}

function Wait-ForTcpPort {
    param(
        [Parameter(Mandatory=$true)][int]$Port,
        [int]$TimeoutSec = 10
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        try {
            $connections = Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue
            if ($connections) { return $true }
        }
        catch {
            # Fallback to netstat on older systems
            $netstat = & netstat -ano 2>$null
            if ($netstat -match ":$Port\s") { return $true }
        }

        if ($script:ClientProcess -and $script:ClientProcess.HasExited) {
            Fail "The client process exited before port $Port became available."
        }

        Start-Sleep -Milliseconds 300
    }

    return $false
}

function Wait-ForInterface {
    param(
        [Parameter(Mandatory=$true)][string]$Alias,
        [int]$TimeoutSec = 5
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $adapter = Get-NetAdapter -Name $Alias -ErrorAction SilentlyContinue
        if ($adapter) { return $adapter }

        if ($script:Tun2SocksProcess -and $script:Tun2SocksProcess.HasExited) {
            Fail "tun2socks exited before the TUN interface appeared."
        }

        Start-Sleep -Milliseconds 200
    }

    return $null
}

function Configure-TunnelInterface {
    param(
        [Parameter(Mandatory=$true)][string]$Alias
    )

    $adapter = Get-NetAdapter -Name $Alias -ErrorAction SilentlyContinue
    if (-not $adapter) {
        Fail "The TUN interface '$Alias' was not found."
    }

    $script:TunnelIfIndex = $adapter.ifIndex

    try {
        Set-NetIPInterface -InterfaceIndex $script:TunnelIfIndex -Dhcp Disabled -ErrorAction SilentlyContinue | Out-Null
    }
    catch {}

    try {
        Get-NetIPAddress -InterfaceIndex $script:TunnelIfIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue |
            Where-Object { $_.IPAddress -eq $TunnelIPv4Address } |
            ForEach-Object { Remove-NetIPAddress -InputObject $_ -Confirm:$false -ErrorAction SilentlyContinue | Out-Null }
    }
    catch {}

    try {
        Get-NetIPAddress -InterfaceIndex $script:TunnelIfIndex -AddressFamily IPv6 -ErrorAction SilentlyContinue |
            Where-Object { $_.IPAddress -eq $TunnelIPv6Address } |
            ForEach-Object { Remove-NetIPAddress -InputObject $_ -Confirm:$false -ErrorAction SilentlyContinue | Out-Null }
    }
    catch {}

    try {
        New-NetIPAddress -InterfaceIndex $script:TunnelIfIndex `
            -IPAddress $TunnelIPv4Address `
            -PrefixLength $TunnelIPv4PrefixLength `
            -AddressFamily IPv4 `
            -ErrorAction SilentlyContinue | Out-Null
    }
    catch {
        Write-Warn "Failed to set IPv4 address on $Alias"
    }

    try {
        New-NetIPAddress -InterfaceIndex $script:TunnelIfIndex `
            -IPAddress $TunnelIPv6Address `
            -PrefixLength $TunnelIPv6PrefixLength `
            -AddressFamily IPv6 `
            -ErrorAction SilentlyContinue | Out-Null
    }
    catch {
        Write-Warn "Failed to set IPv6 address on $Alias"
    }

    Remove-NetRoute -DestinationPrefix '0.0.0.0/0' -InterfaceIndex $script:TunnelIfIndex -Confirm:$false -ErrorAction SilentlyContinue | Out-Null
    Remove-NetRoute -DestinationPrefix '::/0' -InterfaceIndex $script:TunnelIfIndex -Confirm:$false -ErrorAction SilentlyContinue | Out-Null

    try {
        New-NetRoute -InterfaceIndex $script:TunnelIfIndex -DestinationPrefix '0.0.0.0/0' -NextHop '0.0.0.0' -RouteMetric 1 -ErrorAction Stop | Out-Null
    }
    catch {
        Fail "Failed to add the IPv4 default route via $Alias."
    }

    try {
        New-NetRoute -InterfaceIndex $script:TunnelIfIndex -DestinationPrefix '::/0' -NextHop '::' -RouteMetric 1 -ErrorAction SilentlyContinue | Out-Null
    }
    catch {
        Write-Warn "Failed to add IPv6 default route via $Alias"
    }

    try {
        Set-NetIPInterface -InterfaceIndex $script:TunnelIfIndex -InterfaceMetric 1 -ErrorAction SilentlyContinue | Out-Null
    }
    catch {}

    $script:TunConfigured = $true
    $script:RoutesConfigured = $true
    Write-Ok "Tunnel interface '$Alias' is configured."
}

function Cleanup {
    $exitCode = $LASTEXITCODE

    Write-Host ''
    Write-Log 'Shutting down...'

    foreach ($ip in @($script:BypassIPv4 + $script:BypassIPv6)) {
        try { Remove-BypassRoute -IP $ip } catch {}
    }

    if ($script:RoutesConfigured -and $script:TunnelIfIndex) {
        try { Remove-NetRoute -DestinationPrefix '0.0.0.0/0' -InterfaceIndex $script:TunnelIfIndex -Confirm:$false -ErrorAction SilentlyContinue | Out-Null } catch {}
        try { Remove-NetRoute -DestinationPrefix '::/0' -InterfaceIndex $script:TunnelIfIndex -Confirm:$false -ErrorAction SilentlyContinue | Out-Null } catch {}
    }

    if ($script:TunConfigured -and $script:TunnelIfIndex) {
        try {
            $adapter = Get-NetAdapter -InterfaceIndex $script:TunnelIfIndex -ErrorAction SilentlyContinue
            if ($adapter) {
                try { Disable-NetAdapter -Name $adapter.Name -Confirm:$false -ErrorAction SilentlyContinue | Out-Null } catch {}
            }
        }
        catch {}
    }

    if ($script:Tun2SocksProcess -and -not $script:Tun2SocksProcess.HasExited) {
        try {
            Stop-Process -Id $script:Tun2SocksProcess.Id -Force -ErrorAction SilentlyContinue
            Write-Log "Stopped tun2socks (pid $($script:Tun2SocksProcess.Id))"
        }
        catch {}
    }

    if ($script:ClientProcess -and -not $script:ClientProcess.HasExited) {
        try {
            Stop-Process -Id $script:ClientProcess.Id -Force -ErrorAction SilentlyContinue
            Write-Log "Stopped client (pid $($script:ClientProcess.Id))"
        }
        catch {}
    }

    Write-Ok 'Cleanup complete.'
    if ($exitCode -ne $null) { exit $exitCode }
}

try {
    if (-not (Test-Path $ClientPath)) {
        Fail "Client binary not found: $ClientPath"
    }

    if (-not (Test-Path $Tun2SocksPath)) {
        Fail "tun2socks binary not found: $Tun2SocksPath"
    }

    $physical4 = Get-PhysicalGatewayInfo -AddressFamily IPv4
    if (-not $physical4) {
        Fail 'Could not detect an IPv4 physical default route.'
    }

    $script:PhysicalIfIndex = $physical4.IfIndex
    $script:PhysicalGateway4 = $physical4.Gateway
    $script:PhysicalIfAlias = $physical4.Alias

    Write-Log "Physical IPv4 route detected on '$($script:PhysicalIfAlias)' via $($script:PhysicalGateway4)"

    $physical6 = Get-PhysicalGatewayInfo -AddressFamily IPv6
    if ($physical6) {
        $script:PhysicalIfIndex6 = $physical6.IfIndex
        $script:PhysicalGateway6 = $physical6.Gateway
        Write-Log "Physical IPv6 route detected on '$($physical6.Alias)' via $($script:PhysicalGateway6)"
    }
    else {
        Write-Warn 'No IPv6 physical default route found; IPv6 bypass routes will be skipped.'
    }

    Write-Log "Resolving server domain for bypass: $ServerHost"
    Sync-BypassRoutesV4 -HostName $ServerHost -IfIndex $script:PhysicalIfIndex -NextHop $script:PhysicalGateway4

    if ($physical6) {
        Sync-BypassRoutesV6 -HostName $ServerHost -IfIndex $script:PhysicalIfIndex6 -NextHop $script:PhysicalGateway6
    }

    Write-Ok 'Routing isolation is ready.'

    $clientWorkDir = Split-Path -Parent $ClientPath
    if ([string]::IsNullOrWhiteSpace($clientWorkDir)) {
        $clientWorkDir = (Get-Location).Path
    }

    Write-Log "Starting client: $ClientPath $($ClientArgs -join ' ')"
    $script:ClientProcess = Start-Process -FilePath $ClientPath -ArgumentList $ClientArgs -WorkingDirectory $clientWorkDir -PassThru -WindowStyle Hidden

    Start-Sleep -Milliseconds 300
    if ($script:ClientProcess.HasExited) {
        Fail 'The client process exited immediately. Check the client arguments.'
    }

    Write-Log "Waiting for SOCKS5 on $SocksHost:$SocksPort..."
    if (-not (Wait-ForTcpPort -Port $SocksPort -TimeoutSec 10)) {
        Fail "SOCKS5 port $SocksPort did not open within 10 seconds."
    }
    Write-Ok "SOCKS5 is listening on port $SocksPort."

    $tun2socksArgs = @(
        '-device', $InterfaceName,
        '-proxy', "socks5://$SocksHost`:$SocksPort",
        '-loglevel', 'error'
    )

    $tunWorkDir = Split-Path -Parent $Tun2SocksPath
    if ([string]::IsNullOrWhiteSpace($tunWorkDir)) {
        $tunWorkDir = (Get-Location).Path
    }

    Write-Log "Starting tun2socks: $Tun2SocksPath $($tun2socksArgs -join ' ')"
    $script:Tun2SocksProcess = Start-Process -FilePath $Tun2SocksPath -ArgumentList $tun2socksArgs -WorkingDirectory $tunWorkDir -PassThru -WindowStyle Hidden

    Write-Log "Waiting for TUN interface '$InterfaceName'..."
    $adapter = Wait-ForInterface -Alias $InterfaceName -TimeoutSec 5
    if (-not $adapter) {
        Fail "The TUN interface '$InterfaceName' did not appear within 5 seconds."
    }

    Configure-TunnelInterface -Alias $InterfaceName

    Write-Host ''
    Write-Host 'All traffic is now routed through the proxy.' -ForegroundColor Green
    Write-Host ("  Physical NIC:   {0} (ifIndex {1})" -f $script:PhysicalIfAlias, $script:PhysicalIfIndex)
    Write-Host ("  Server domain:  {0}" -f $ServerHost)
    Write-Host ("  Bypass IPs:     {0}" -f ((($script:BypassIPv4 + $script:BypassIPv6) -join ', ') -replace '^$', '(none)'))
    Write-Host ("  SOCKS5:         {0}:{1}" -f $SocksHost, $SocksPort)
    Write-Host ("  TUN interface:  {0}" -f $InterfaceName)
    Write-Host ("  Client PID:     {0}" -f $script:ClientProcess.Id)
    Write-Host ("  tun2socks PID:  {0}" -f $script:Tun2SocksProcess.Id)
    Write-Host ("  Refresh every:  {0} seconds" -f $RefreshIntervalSec)
    Write-Host ''
    Write-Host 'Press Ctrl+C to stop and clean up.' -ForegroundColor White
    Write-Host ''

    while ($true) {
        if ($script:ClientProcess.HasExited) {
            Fail 'The client process exited.'
        }

        if ($script:Tun2SocksProcess.HasExited) {
            Fail 'tun2socks exited.'
        }

        Sync-BypassRoutesV4 -HostName $ServerHost -IfIndex $script:PhysicalIfIndex -NextHop $script:PhysicalGateway4

        if ($physical6 -and $script:PhysicalIfIndex6 -and $script:PhysicalGateway6) {
            Sync-BypassRoutesV6 -HostName $ServerHost -IfIndex $script:PhysicalIfIndex6 -NextHop $script:PhysicalGateway6
        }

        Start-Sleep -Seconds $RefreshIntervalSec
    }
}
finally {
    Cleanup
}
