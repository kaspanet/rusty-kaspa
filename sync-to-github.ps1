# Sync rusty-kaspa source code to GitHub location
# Excludes: target/, stratum-proxy/, and other build artifacts

param(
    [string]$SourcePath = "E:\mixer1.0-main\mixer1.0-main\rusty-kaspa",
    [string]$TargetPath = "E:\rusty-kaspa",
    [switch]$DryRun = $false
)

Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host "  Sync rusty-kaspa to GitHub Location" -ForegroundColor Cyan
Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host ""
Write-Host "Source: $SourcePath"
Write-Host "Target: $TargetPath"
Write-Host "Dry Run: $DryRun"
Write-Host ""

# Verify source exists
if (-not (Test-Path $SourcePath)) {
    Write-Host "ERROR: Source path does not exist: $SourcePath" -ForegroundColor Red
    exit 1
}

# Create target if it doesn't exist
if (-not (Test-Path $TargetPath)) {
    Write-Host "Creating target directory: $TargetPath" -ForegroundColor Yellow
    if (-not $DryRun) {
        New-Item -ItemType Directory -Path $TargetPath -Force | Out-Null
    }
}

# Directories/files to exclude
$excludeDirs = @("target", "stratum-proxy", "kaspad-status", ".git")
$excludeFiles = @() # Add specific files if needed

Write-Host "Excluding directories:" -ForegroundColor Yellow
foreach ($dir in $excludeDirs) {
    Write-Host "  - $dir/"
}

Write-Host ""
Write-Host "Starting sync..." -ForegroundColor Green
Write-Host ""

# Function to check if path should be excluded
function Should-Exclude {
    param([string]$Path)
    
    foreach ($exclude in $excludeDirs) {
        if ($Path -like "*\$exclude\*" -or $Path -like "*\$exclude") {
            return $true
        }
    }
    return $false
}

# Get all items from source
$items = Get-ChildItem -Path $SourcePath -Recurse -Force | Where-Object {
    -not (Should-Exclude $_.FullName)
}

$totalItems = $items.Count
$copiedItems = 0
$skippedItems = 0
$errors = 0

foreach ($item in $items) {
    $relativePath = $item.FullName.Substring($SourcePath.Length + 1)
    $targetItemPath = Join-Path $TargetPath $relativePath
    
    try {
        if ($item.PSIsContainer) {
            # Directory
            if (-not (Test-Path $targetItemPath)) {
                if (-not $DryRun) {
                    New-Item -ItemType Directory -Path $targetItemPath -Force | Out-Null
                }
                Write-Host "[DIR]  $relativePath" -ForegroundColor Cyan
            }
        } else {
            # File
            $shouldCopy = $true
            
            # Check if file exists and is newer
            if (Test-Path $targetItemPath) {
                $sourceTime = $item.LastWriteTime
                $targetTime = (Get-Item $targetItemPath).LastWriteTime
                if ($targetTime -ge $sourceTime) {
                    $shouldCopy = $false
                    $skippedItems++
                }
            }
            
            if ($shouldCopy) {
                if (-not $DryRun) {
                    $targetDir = Split-Path $targetItemPath -Parent
                    if (-not (Test-Path $targetDir)) {
                        New-Item -ItemType Directory -Path $targetDir -Force | Out-Null
                    }
                    Copy-Item -Path $item.FullName -Destination $targetItemPath -Force
                }
                Write-Host "[FILE] $relativePath" -ForegroundColor Green
                $copiedItems++
            } else {
                Write-Host "[SKIP] $relativePath (up to date)" -ForegroundColor Gray
            }
        }
    } catch {
        Write-Host "[ERROR] $relativePath : $($_.Exception.Message)" -ForegroundColor Red
        $errors++
    }
    
    # Progress indicator
    if (($copiedItems + $skippedItems) % 100 -eq 0) {
        $progress = [math]::Round((($copiedItems + $skippedItems) / $totalItems) * 100, 1)
        Write-Host "Progress: $progress% ($($copiedItems + $skippedItems)/$totalItems)" -ForegroundColor Yellow
    }
}

Write-Host ""
Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host "  Sync Complete" -ForegroundColor Cyan
Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host ""
Write-Host "Total items processed: $totalItems"
Write-Host "Copied: $copiedItems" -ForegroundColor Green
Write-Host "Skipped (up to date): $skippedItems" -ForegroundColor Gray
Write-Host "Errors: $errors" -ForegroundColor $(if ($errors -gt 0) { "Red" } else { "Green" })
Write-Host ""

if ($DryRun) {
    Write-Host "This was a DRY RUN - no files were actually copied." -ForegroundColor Yellow
    Write-Host "Run without -DryRun to perform the actual sync." -ForegroundColor Yellow
} else {
    Write-Host "Files synced successfully!" -ForegroundColor Green
    Write-Host ""
    Write-Host "Next steps:" -ForegroundColor Cyan
    Write-Host "1. Review changes in: $TargetPath"
    Write-Host "2. Check git status: cd $TargetPath; git status"
    Write-Host "3. Commit changes: git add .; git commit -m 'Sync source code'"
}

