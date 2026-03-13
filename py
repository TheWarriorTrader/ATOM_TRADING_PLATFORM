#!/bin/bash
# Wrapper script per Python nel virtual environment del progetto
# Uso: ./py [args...]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
"${SCRIPT_DIR}/.venv/bin/python" "$@"
