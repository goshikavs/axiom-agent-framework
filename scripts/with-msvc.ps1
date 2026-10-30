# Load the x64 MSVC environment and run the remaining arguments.
# Required on Windows when link.exe is not on PATH (typical for Build Tools).
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Command
)
$vcvars = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvarsall.bat"
if (-not (Test-Path $vcvars)) {
    $vcvars = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat"
}
if (-not (Test-Path $vcvars)) {
    Write-Error "vcvarsall.bat not found. Install Visual Studio Build Tools with the C++ workload."
    exit 1
}
$quoted = ($Command | ForEach-Object { if ($_ -match '\s') { '"' + $_ + '"' } else { $_ } }) -join ' '
cmd /c "call `"$vcvars`" x64 >nul && $quoted"
exit $LASTEXITCODE
