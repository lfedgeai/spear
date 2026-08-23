# Disabling spear-next in Top-Level Makefile

## Overview

This document records how to disable the spear-next subproject from the build process in the top-level Makefile.

## Background

The top-level Makefile uses an automatic discovery mechanism to find all subdirectories containing Makefiles and includes them in the build process. This could cause the spear-next project to be unintentionally included in the main project's build workflow.

## Solution

### Before
```makefile
SUBDIRS := $(shell find $(REPO_ROOT) -mindepth 1 -maxdepth 3 -type d -exec test -e {}/Makefile \; -exec echo {} \;)
```

### After
```makefile
SUBDIRS := $(shell find $(REPO_ROOT) -mindepth 1 -maxdepth 3 -type d -exec test -e {}/Makefile \; -exec echo {} \; | grep -v spear-next)
```

## Changes

1. **Filtering Mechanism**: Added `| grep -v spear-next` filter to the `SUBDIRS` variable definition
2. **Excluded Directory**: The `spear-next` directory is now excluded from the automatic build workflow
3. **Compatibility**: Build workflows for other subprojects remain unchanged

## Affected Targets

The following Makefile targets no longer include spear-next:

- `clean`: Cleanup operations no longer include spear-next
- `build`: Build operations no longer include spear-next  
- `test`: Test operations no longer include spear-next

## Verification

### Check Current Included Subdirectories
```bash
find . -mindepth 1 -maxdepth 3 -type d -exec test -e {}/Makefile \; -exec echo {} \; | grep -v spear-next
```

### Preview Build Process
```bash
make -n all
```

## Notes

1. **Independent Build**: spear-next can still be built independently by entering its directory and running `make`
2. **CI/CD Impact**: If CI/CD workflows depend on the top-level Makefile, they may need corresponding adjustments
3. **Restoration**: To re-enable, simply remove the `| grep -v spear-next` filter

## Related Files

- `/Makefile` - Top-level Makefile
- `/spear-next/Makefile` - spear-next project Makefile (still exists and usable)

## Date

Modified: January 2024
