#!/bin/bash
# Reproduction script for https://github.com/joemckenney/wake/issues/10
# Issue: No commands recorded on immutable Linux OS (Fedora Atomic)
#
# Root cause: SHELL env var is not exported on Fedora, so wake falls back
# to /bin/sh which doesn't source .bashrc, so hooks never load.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

# Create Dockerfile
DOCKERFILE=$(mktemp)
cat > "$DOCKERFILE" << 'DOCKERFILE_CONTENT'
FROM fedora:43

# Install utilities
RUN dnf install -y util-linux-script which && dnf clean all

# Mimic Fedora Atomic home directory structure  
RUN mkdir -p /var/home/testuser && \
    rm -rf /home && \
    ln -s /var/home /home && \
    useradd -d /var/home/testuser -s /bin/bash testuser && \
    chown -R testuser:testuser /var/home/testuser

# Install wake binary
COPY target/release/wake /usr/local/bin/wake
RUN chmod +x /usr/local/bin/wake

# Set up bashrc with wake hooks  
USER testuser
WORKDIR /var/home/testuser
RUN echo 'eval "$(wake init bash)"' >> ~/.bashrc

CMD ["/bin/bash"]
DOCKERFILE_CONTENT

echo "=== Building container ==="
docker build -f "$DOCKERFILE" -t wake-fedora-test "$REPO_ROOT"

echo ""
echo "=== Test 1: Confirm SHELL is not exported ==="
docker run --rm wake-fedora-test bash -c '
  echo "SHELL variable: $SHELL"
  echo "SHELL in env:   $(printenv SHELL || echo "NOT EXPORTED")"
'

echo ""
echo "=== Test 2: Reproduce the issue ==="
docker run --rm wake-fedora-test script -q -c '
  wake shell << INNER
    echo "Running inside wake shell..."
    echo "Hook defined: \$(type __wake_preexec 2>/dev/null && echo YES || echo NO)"
    ls /tmp > /dev/null
    exit
INNER
  echo ""
  echo "Commands recorded:"
  wake log -c 3
' /dev/null 2>&1 | strings

echo ""
echo "=== Test 3: Confirm workaround (export SHELL) ==="
docker run --rm wake-fedora-test script -q -c '
  export SHELL  # THE FIX
  wake shell << INNER
    echo "Hook defined: \$(type __wake_preexec 2>/dev/null && echo YES || echo NO)"
    ls /tmp > /dev/null
    exit
INNER
  echo ""
  echo "Commands recorded:"
  wake log -c 3
' /dev/null 2>&1 | strings

# Cleanup
rm -f "$DOCKERFILE"

echo ""
echo "=== Done ==="
