# Adds or removes one folder in the current user's Path. Called by ttoryeot-setup.nsi.
# usage: path.ps1 add|remove   (the folder comes from $env:TTORYEOT_PATH_DIR)
#
# NSIS strings are limited to 1024 characters, so reading and rewriting Path inside NSIS can
# silently cut a long Path. This script works on the registry value directly instead:
# - it reads HKCU\Environment\Path without expanding %VARS% and keeps the value kind
#   (REG_EXPAND_SZ stays REG_EXPAND_SZ),
# - "add" appends one entry and leaves the existing text byte for byte as it was,
# - "remove" drops only the entries that name the install folder and joins the rest unchanged.
# Exit codes: 0 changed, 2 nothing to do, 1 error. The installer broadcasts WM_SETTINGCHANGE.
param([Parameter(Mandatory = $true)][ValidateSet('add', 'remove')][string]$Mode)
$ErrorActionPreference = 'Stop'
try {
    $dir = $env:TTORYEOT_PATH_DIR
    if ([string]::IsNullOrWhiteSpace($dir)) { throw 'TTORYEOT_PATH_DIR is empty' }
    $want = $dir.Trim().TrimEnd('\')
    function Same([string]$entry) {
        $e = [Environment]::ExpandEnvironmentVariables($entry.Trim()).TrimEnd('\')
        return [string]::Equals($e, $want, [StringComparison]::OrdinalIgnoreCase)
    }

    $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
    try {
        $exists = $key.GetValueNames() -contains 'Path'
        if ($exists) {
            $old = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
            $kind = $key.GetValueKind('Path')
        } else {
            $old = ''
            $kind = [Microsoft.Win32.RegistryValueKind]::ExpandString
        }
        $parts = if ($old.Length -gt 0) { $old.Split(';') } else { @() }

        if ($Mode -eq 'add') {
            foreach ($p in $parts) { if (Same $p) { exit 2 } }
            if ($old.Length -eq 0) { $new = $want }
            elseif ($old.EndsWith(';')) { $new = $old + $want }
            else { $new = $old + ';' + $want }
            $key.SetValue('Path', $new, $kind)
            exit 0
        }

        if (-not $exists) { exit 2 }
        $keep = @($parts | Where-Object { -not (Same $_) })
        if ($keep.Count -eq $parts.Count) { exit 2 }
        $new = [string]::Join(';', $keep)
        if ($new.Trim(';').Length -eq 0) { $key.DeleteValue('Path') }
        else { $key.SetValue('Path', $new, $kind) }
        exit 0
    } finally {
        $key.Close()
    }
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
