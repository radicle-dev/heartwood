$prefix = "releases"

$describe = git describe --match "$prefix/*.*.*" --exclude "$prefix/*.*.*-*.*" --candidates=1 2>$null
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($describe)) {
  Write-Error -Category ObjectNotFound "tag not found"
  exit 1
}

if (-not ($describe.Trim() -match "^$prefix/(\d+)\.(\d+)\.(\d+)(?:-(\d+)-g[0-9a-f]+)?$")) {
  Write-Error -Category ParserError "tag not recognized"
  exit 1
}

$major = [int]$matches[1]
$minor = [int]$matches[2]
$patch = [int]$matches[3]

$commits = if ([string]::IsNullOrEmpty($matches[4])) { 0 } else { [int]$matches[4] }

Write-Output "$major.$minor.$patch.$commits"
