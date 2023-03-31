#!/bin/bash

# Check if the script is called with an argument
if [ $# -ne 1 ]; then
    echo "Usage: $0 <directory>"
    exit 1
fi

# Save the argument as a variable
directory="$1"

# Get the latest Git hash for the current working directory
git_hash=$(git rev-parse HEAD)

# Navigate to the target release directory and copy the 'seaborg' file
cargo build --release
cp target/release/seaborg "$directory/seaborg-$git_hash"
