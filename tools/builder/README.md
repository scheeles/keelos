# Builder

The `builder` directory provides a consistent build environment for MaticOS, packaged as a Docker container.

Since MaticOS is a Linux distribution, it must be built on Linux. This builder ensures that macOS and Windows users can build the OS correctly.

## Usage

1.  **Start the Builder Shell**:
    ```bash
    ./tools/builder/build.sh
    ```
    This drops you into a shell inside the container with tools (`cargo`, `make`, `gcc`) installed.

2.  **Inside the Container**:
    You can run build scripts (to be created) like:
    ```bash
    # (Future)
    # cargo build --release
    # ./tools/builder/kernel.sh
    ```
