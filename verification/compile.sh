#!/bin/bash
# Exit on error, undefined variables, and pipe failures
set -euo pipefail

trap 'echo "Compilation failed on line ${LINENO}: ${BASH_COMMAND}"' ERR

# Get javac version information
JAVAC_VERSION=$(javac -version 2>&1)

# Extract compiler name and version
if [[ "$JAVAC_VERSION" == *"javac "* ]]; then
    COMPILER_NAME="openjdk"
    COMPILER_VERSION=$(echo "$JAVAC_VERSION" | sed 's/javac //' | sed 's/\..*//')
else
    COMPILER_NAME="unknown"
    COMPILER_VERSION="unknown"
fi

# Create output directory name
OUTPUT_DIR="/data/projects/rajac/verification/output/${COMPILER_NAME}_${COMPILER_VERSION}"

rm -rf "$OUTPUT_DIR"
# Create output directory
mkdir -p "$OUTPUT_DIR"

# Compile all Java files recursively
find /data/projects/rajac/verification/sources -name "*.java" -type f | xargs javac -d "$OUTPUT_DIR"

echo "Compiled Java files to: $OUTPUT_DIR"

# Compile invalid Java files separately and capture output
INVALID_OUTPUT_DIR="/data/projects/rajac/verification/output/${COMPILER_NAME}_${COMPILER_VERSION}/invalid"
mkdir -p "$INVALID_OUTPUT_DIR"
INVALID_OUTPUT_FILE="${INVALID_OUTPUT_DIR}/errors.txt"
: > "$INVALID_OUTPUT_FILE"

INVALID_COUNT=0
for STAGE in lexer parser typecheck; do
    STAGE_DIR="/data/projects/rajac/verification/sources_invalid/${STAGE}"
    if [[ ! -d "$STAGE_DIR" ]]; then
        continue
    fi

    mapfile -t STAGE_FILES < <(find "$STAGE_DIR" -name "*.java" -type f | sort)
    STAGE_COUNT="${#STAGE_FILES[@]}"
    INVALID_COUNT=$((INVALID_COUNT + STAGE_COUNT))

    if [[ "$STAGE_COUNT" -eq 0 ]]; then
        continue
    fi

    if javac -d "$INVALID_OUTPUT_DIR" "${STAGE_FILES[@]}" >> "$INVALID_OUTPUT_FILE" 2>&1; then
        echo "ERROR: javac succeeded but should have failed for invalid ${STAGE} sources"
        exit 1
    fi
done

echo "Compiled invalid Java files to: $INVALID_OUTPUT_FILE"
echo "Number of valid files: $(find /data/projects/rajac/verification/sources -name "*.java" -type f | wc -l)"
echo "Number of invalid files: $INVALID_COUNT"
