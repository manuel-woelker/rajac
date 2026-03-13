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

INVALID_COUNT=0
while read -r java_file; do
    output_file="${INVALID_OUTPUT_DIR}/$(basename "$java_file" .java).txt"
    if javac -d "$INVALID_OUTPUT_DIR" "$java_file" > "$output_file" 2>&1; then
        echo "ERROR: javac succeeded but should have failed for: $java_file"
        exit 1
    fi
    ((INVALID_COUNT++)) || true
done < <(find /data/projects/rajac/verification/sources_invalid -name "*.java" -type f)

echo "Compiled invalid Java files to: $INVALID_OUTPUT_DIR"
echo "Number of valid files: $(find /data/projects/rajac/verification/sources -name "*.java" -type f | wc -l)"
echo "Number of invalid files: $INVALID_COUNT"