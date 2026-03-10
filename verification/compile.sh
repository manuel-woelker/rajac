#!/bin/bash

# Get javac version information
JAVAC_VERSION=$($JAVA_HOME/bin/javac -version 2>&1)

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
find /data/projects/rajac/verification/sources -name "*.java" -type f | xargs $JAVA_HOME/bin/javac -d "$OUTPUT_DIR"

echo "Compiled Java files to: $OUTPUT_DIR"