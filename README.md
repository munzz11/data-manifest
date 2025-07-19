# Data Manifest Generator

A high-performance Rust tool for generating SHA256 checksums for all files in a data archive. Designed for speed with parallel processing and optimized for Ubuntu 20.04 environments.

## Features

- **Parallel Processing**: Uses all available CPU cores for maximum speed
- **Progress Tracking**: Optional progress bar with ETA
- **Configurable Buffer Size**: Optimize for your storage system
- **Docker Support**: Ready-to-use containerized version
- **Cross-Platform**: Works on Linux, macOS, and Windows

## Output Format

Each line in the output file contains:
```
<sha256_checksum> <file_path>
```

Example:
```
a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef12345678 /archive/data/file1.txt
```

## Local Development

### Prerequisites

- Rust 1.75 or later
- Cargo

### Building

```bash
cargo build --release
```

### Usage

```bash
# Basic usage
./target/release/data-manifest --archive-path /path/to/archive --output manifest.txt

# With progress bar
./target/release/data-manifest --archive-path /path/to/archive --output manifest.txt --progress

# Custom thread count and buffer size
./target/release/data-manifest \
    --archive-path /path/to/archive \
    --output manifest.txt \
    --threads 8 \
    --buffer-size 2097152 \
    --progress
```

### Command Line Options

- `-a, --archive-path <PATH>`: Path to the archive directory (required)
- `-o, --output <FILE>`: Output file for the manifest (default: manifest.txt)
- `-t, --threads <NUM>`: Number of worker threads (default: number of CPU cores)
- `-b, --buffer-size <BYTES>`: Buffer size for reading files (default: 1048576 bytes)
- `-p, --progress`: Show progress bar
- `-h, --help`: Show help information

## Docker Usage

### Building the Container

```bash
docker build -t data-manifest .
```

### Running with Docker

```bash
# Basic usage
docker run -v /path/to/archive:/archive -v /path/to/output:/output data-manifest \
    --archive-path /archive --output /output/manifest.txt

# With progress bar
docker run -v /path/to/archive:/archive -v /path/to/output:/output data-manifest \
    --archive-path /archive --output /output/manifest.txt --progress

# Custom configuration
docker run -v /path/to/archive:/archive -v /path/to/output:/output data-manifest \
    --archive-path /archive \
    --output /output/manifest.txt \
    --threads 8 \
    --buffer-size 2097152 \
    --progress
```

### Docker Volume Mounts

- `/archive`: Mount your data archive directory here
- `/output`: Mount the directory where you want the manifest file saved

## Performance Tips

1. **SSD Storage**: For best performance, ensure both the archive and output are on SSD storage
2. **Memory**: The tool uses configurable buffer sizes. For large files, increase the buffer size
3. **CPU Cores**: The tool automatically uses all available CPU cores. For I/O bound workloads, you may want to reduce thread count
4. **Network Storage**: For network-mounted archives, consider using larger buffer sizes

## Example Performance

On a typical system with SSD storage:
- **Small archive** (1GB, 1000 files): ~10-30 seconds
- **Medium archive** (100GB, 10000 files): ~5-15 minutes
- **Large archive** (1TB, 100000 files): ~1-3 hours

Actual performance depends on:
- Storage type (SSD vs HDD vs network)
- File sizes and count
- Available CPU cores
- System memory

## Error Handling

The tool continues processing even if individual files fail to hash. Errors are reported to stderr, and the final summary shows success/error counts.

Common error scenarios:
- Permission denied
- File not found (deleted during processing)
- Disk full
- Corrupted files

## License

See LICENSE file for details. 