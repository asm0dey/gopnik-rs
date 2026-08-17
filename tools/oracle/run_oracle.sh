#!/bin/sh
# Ground-truth oracle: run the original with a keystroke script and print the
# screen it ended on.
#
#   tools/oracle/run_oracle.sh '<keys>' <out_dir>
#
# \n in <keys> is Enter. Every captured frame is left in <out_dir>/screens.txt
# and the raw text-mode buffers (characters and attributes) in
# <out_dir>/work/SCREEN.BIN. See docs/re/oracle.md.
set -eu

if [ $# -ne 2 ]; then
    echo "usage: $0 '<keys>' <out_dir>" >&2
    exit 2
fi

exec python3 "$(dirname "$0")/capture.py" --keys "$1" --out "$2"
