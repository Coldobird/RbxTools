[CmdletBinding()]
param(
    [switch]$NoUi
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:RuleGroup = 'RbxTools - Steam Block'

function Test-IsAdministrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Get-SteamExecutablePaths {
    $installRoots = [System.Collections.Generic.HashSet[string]]::new(
        [StringComparer]::OrdinalIgnoreCase
    )

    $registryKeys = @(
        'HKCU:\Software\Valve\Steam',
        'HKLM:\SOFTWARE\WOW6432Node\Valve\Steam',
        'HKLM:\SOFTWARE\Valve\Steam'
    )

    foreach ($key in $registryKeys) {
        if (Test-Path -LiteralPath $key) {
            $properties = Get-ItemProperty -LiteralPath $key
            foreach ($propertyName in @('SteamPath', 'InstallPath')) {
                $property = $properties.PSObject.Properties[$propertyName]
                $value = if ($property) { $property.Value } else { $null }
                if ($value) {
                    [void]$installRoots.Add([Environment]::ExpandEnvironmentVariables($value))
                }
            }
        }
    }

    if (${env:ProgramFiles(x86)}) {
        [void]$installRoots.Add((Join-Path ${env:ProgramFiles(x86)} 'Steam'))
    }

    $relativeExecutables = @(
        'steam.exe',
        'steamwebhelper.exe',
        'bin\cef\cef.win7x64\steamwebhelper.exe',
        'bin\cef\cef.win7\steamwebhelper.exe'
    )

    $results = [System.Collections.Generic.HashSet[string]]::new(
        [StringComparer]::OrdinalIgnoreCase
    )

    foreach ($root in $installRoots) {
        foreach ($relativePath in $relativeExecutables) {
            $candidate = Join-Path $root $relativePath
            if (Test-Path -LiteralPath $candidate -PathType Leaf) {
                [void]$results.Add((Resolve-Path -LiteralPath $candidate).Path)
            }
        }
    }

    Get-Process -Name 'steam', 'steamwebhelper' -ErrorAction SilentlyContinue |
        ForEach-Object {
            try {
                if ($_.Path -and (Test-Path -LiteralPath $_.Path -PathType Leaf)) {
                    [void]$results.Add($_.Path)
                }
            }
            catch {
                # Alguns processos protegidos não expõem Path sem elevação.
            }
        }

    return @($results | Sort-Object)
}

function Set-SteamNetworkBlocked {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string[]]$ExecutablePaths
    )

    if (-not (Test-IsAdministrator)) {
        throw 'Esta operação requer permissão de administrador.'
    }

    Import-Module NetSecurity -ErrorAction Stop

    $validPaths = @($ExecutablePaths | Where-Object {
        $_ -and (Test-Path -LiteralPath $_ -PathType Leaf) -and
        ([IO.Path]::GetExtension($_) -ieq '.exe')
    } | Sort-Object -Unique)

    if ($validPaths.Count -eq 0) {
        throw 'Nenhum executável válido foi selecionado.'
    }

    Remove-SteamNetworkBlock

    foreach ($path in $validPaths) {
        $leaf = Split-Path -Leaf $path
        foreach ($direction in @('Inbound', 'Outbound')) {
            $displayName = "RbxTools - Bloquear Steam - $leaf - $direction"
            New-NetFirewallRule `
                -DisplayName $displayName `
                -Group $script:RuleGroup `
                -Direction $direction `
                -Program $path `
                -Action Block `
                -Profile Any `
                -Enabled True | Out-Null
        }
    }
}

function Remove-SteamNetworkBlock {
    if (-not (Test-IsAdministrator)) {
        throw 'Esta operação requer permissão de administrador.'
    }

    Import-Module NetSecurity -ErrorAction Stop
    Get-NetFirewallRule -Group $script:RuleGroup -ErrorAction SilentlyContinue |
        Remove-NetFirewallRule -ErrorAction Stop
}

function Get-SteamNetworkBlockStatus {
    Import-Module NetSecurity -ErrorAction Stop
    $rules = @(Get-NetFirewallRule -Group $script:RuleGroup -ErrorAction SilentlyContinue)
    return [pscustomobject]@{
        IsBlocked = $rules.Count -gt 0
        RuleCount = $rules.Count
    }
}

function Start-ElevatedCopy {
    if (Test-IsAdministrator) {
        return $false
    }

    if (-not $PSCommandPath) {
        throw 'Não foi possível localizar o arquivo do aplicativo.'
    }

    Start-Process -FilePath 'powershell.exe' -Verb RunAs -ArgumentList @(
        '-NoProfile',
        '-ExecutionPolicy', 'Bypass',
        '-STA',
        '-File', ('"{0}"' -f $PSCommandPath)
    )
    return $true
}

function Show-RbxToolsWindow {
    Add-Type -AssemblyName PresentationFramework

    [xml]$xaml = @'
<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
        Title="RbxTools" Height="470" Width="720"
        WindowStartupLocation="CenterScreen" ResizeMode="CanMinimize"
        Background="#10131A" Foreground="#F5F7FA">
  <Grid Margin="28">
    <Grid.RowDefinitions>
      <RowDefinition Height="Auto"/>
      <RowDefinition Height="Auto"/>
      <RowDefinition Height="*"/>
      <RowDefinition Height="Auto"/>
      <RowDefinition Height="Auto"/>
    </Grid.RowDefinitions>

    <TextBlock Text="RbxTools" FontSize="30" FontWeight="Bold"/>
    <TextBlock Grid.Row="1" Margin="0,8,0,18" Foreground="#AEB7C6"
               Text="Controle de rede da Steam — bloqueio de entrada e saída"/>

    <Border Grid.Row="2" Background="#181D27" CornerRadius="8" Padding="14">
      <Grid>
        <Grid.RowDefinitions>
          <RowDefinition Height="Auto"/>
          <RowDefinition Height="*"/>
        </Grid.RowDefinitions>
        <TextBlock Text="Executáveis detectados" FontWeight="SemiBold" Margin="0,0,0,10"/>
        <ListBox Name="ExecutableList" Grid.Row="1" Background="#0D1016"
                 Foreground="#F5F7FA" BorderBrush="#30394A"
                 SelectionMode="Extended"/>
      </Grid>
    </Border>

    <TextBlock Name="StatusText" Grid.Row="3" Margin="0,16,0,12"
               Foreground="#77D5A5" Text="Verificando status..."/>

    <StackPanel Grid.Row="4" Orientation="Horizontal" HorizontalAlignment="Right">
      <Button Name="RefreshButton" Content="Atualizar" Padding="18,9" Margin="0,0,10,0"/>
      <Button Name="AllowButton" Content="Liberar Steam" Padding="18,9" Margin="0,0,10,0"/>
      <Button Name="BlockButton" Content="Bloquear Steam" Padding="18,9"
              Background="#E65353" Foreground="White"/>
    </StackPanel>
  </Grid>
</Window>
'@

    $reader = [System.Xml.XmlNodeReader]::new($xaml)
    $window = [Windows.Markup.XamlReader]::Load($reader)
    $list = $window.FindName('ExecutableList')
    $status = $window.FindName('StatusText')
    $refresh = $window.FindName('RefreshButton')
    $allow = $window.FindName('AllowButton')
    $block = $window.FindName('BlockButton')

    $refreshView = {
        $list.Items.Clear()
        foreach ($path in Get-SteamExecutablePaths) {
            [void]$list.Items.Add($path)
        }
        $list.SelectAll()

        $firewallStatus = Get-SteamNetworkBlockStatus
        if ($firewallStatus.IsBlocked) {
            $status.Text = "Steam bloqueada ($($firewallStatus.RuleCount) regras ativas)."
            $status.Foreground = '#FFB86B'
        }
        else {
            $status.Text = 'Steam liberada. Nenhuma regra do RbxTools está ativa.'
            $status.Foreground = '#77D5A5'
        }

        if ($list.Items.Count -eq 0) {
            $status.Text = 'Steam não encontrada. Inicie ou instale a Steam e clique em Atualizar.'
            $status.Foreground = '#FFB86B'
        }
    }

    $refresh.Add_Click({ & $refreshView })
    $block.Add_Click({
        try {
            $selected = @($list.SelectedItems | ForEach-Object { [string]$_ })
            Set-SteamNetworkBlocked -ExecutablePaths $selected
            & $refreshView
        }
        catch {
            [System.Windows.MessageBox]::Show($_.Exception.Message, 'RbxTools') | Out-Null
        }
    })
    $allow.Add_Click({
        try {
            Remove-SteamNetworkBlock
            & $refreshView
        }
        catch {
            [System.Windows.MessageBox]::Show($_.Exception.Message, 'RbxTools') | Out-Null
        }
    })

    & $refreshView
    [void]$window.ShowDialog()
}

if (-not $NoUi) {
    if (-not (Start-ElevatedCopy)) {
        Show-RbxToolsWindow
    }
}
