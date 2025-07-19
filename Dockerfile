# Build and run data-manifest tool in a single stage
FROM rust:1.75-slim

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /app

# Copy the entire project
COPY . .

# Build the application
RUN cargo build --release

# Create mount points for archive and output
RUN mkdir -p /archive /output

# Set the entrypoint to the built binary
ENTRYPOINT ["target/release/data-manifest"]

# Default arguments
CMD ["--help"] 