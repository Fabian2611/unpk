# unpk
A simple CLI tool to unpack any archive, with consistent flattening.

## Why?
Because I keep forgetting the weird syntax differences between tar, unzip, 7z and unrar, I've made a tool that extracts
them all and handles output directories and flattening for me.

## Usage
```
unpk <file> [OPTIONS]

Options:
  -o, --output <dir>   Extract into a specific directory
      --here           Extract into current directory
      --dry-run        Show what would happen without writing anything
      --list           List archive contents without extracting
  -h, --help           Show this help message
```

The corresponding backend (tar, unzip, 7z, unrar) needs to be installed and on PATH.
