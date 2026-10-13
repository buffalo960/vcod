#!/usr/bin/env bash
# Launches the retail 1.1d Linux dedicated server for protocol captures and
# controlled visual tests. COD_VERSION=1.5 runs the 1.5b server instead
# (protocol 6, docs/research/cod15-protocol-diff.md); COD_DIR must then be a
# 1.5 install, since a 1.1 pak0 fails the 1.5 server's checksums.
#
# The server binary, cod_lnxded (the 1.1d Linux dedicated server), is not in the
# repo and has to be obtained separately. It is a 32-bit ELF and needs a 32-bit
# glibc on the host. The engine loads its game module, main/game.mp.i386.so, from
# the homepath, so the homepath must hold the 1.1 MP game module; the game install
# supplies the pk3s through fs_basepath.
#
#   COD_DIR          game install, the directory containing main/ (required)
#   COD_VERSION      1.1 (default) or 1.5
#   COD_LNXDED       server binary (default private/reference/cod-lnxded-1.1d/cod_lnxded,
#                    cod-lnxded-1.5b/cod_lnxded for 1.5)
#   COD_LNXDED_HOME  homepath holding main/game.mp.i386.so (default private/server,
#                    private/server-1.5 for 1.5)
#
# The 1.5b binary links libstdc++.so.5 (gcc 3.2), which current distributions
# no longer ship; the script puts private/reference/cod-lnxded-1.5b/lib on
# LD_LIBRARY_PATH, so drop a 32-bit copy there (Debian's libstdc++5 i386).
#   PORT             UDP port (default 28960)
#
#   tools/run_server.sh [map] [+set cvar value ...]
#     map defaults to mp_carentan; anything after it is passed to the engine
#     verbatim, which takes its +set arguments in any order. The tdm hit
#     capture uses `+set g_gametype tdm +set scr_friendlyfire 1`.
set -euo pipefail
cd "$(dirname "$0")/.."
if [ -z "${COD_DIR:-}" ]; then
    echo "COD_DIR is not set; point it at the game install (the directory containing main/)" >&2
    exit 1
fi
[ -d "$COD_DIR/main" ] || { echo "COD_DIR=$COD_DIR has no main/ directory" >&2; exit 1; }
case "${COD_VERSION:-1.1}" in
    1.1) REF=private/reference/cod-lnxded-1.1d HOME_DEFAULT=private/server ;;
    1.5) REF=private/reference/cod-lnxded-1.5b HOME_DEFAULT=private/server-1.5
         export LD_LIBRARY_PATH="$PWD/$REF/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" ;;
    *) echo "COD_VERSION must be 1.1 or 1.5" >&2; exit 1 ;;
esac
BIN="${COD_LNXDED:-$REF/cod_lnxded}"
HOMEPATH="${COD_LNXDED_HOME:-$HOME_DEFAULT}"
MAP="${1:-mp_carentan}"
[ -x "$BIN" ] || { echo "server binary $BIN missing or not executable; set COD_LNXDED" >&2; exit 1; }
[ -f "$HOMEPATH/main/game.mp.i386.so" ] || {
    echo "no game module at $HOMEPATH/main/game.mp.i386.so; set COD_LNXDED_HOME" >&2; exit 1; }
if [ ! -e /lib/ld-linux.so.2 ]; then
    echo "32-bit loader /lib/ld-linux.so.2 missing; the server needs a 32-bit glibc" >&2
    exit 1
fi
HOMEPATH="$(cd "$HOMEPATH" && pwd)"
exec "$BIN" \
    +set dedicated 1 \
    +set fs_basepath "$COD_DIR" \
    +set fs_homepath "$HOMEPATH" \
    +set net_port "${PORT:-28960}" \
    +set sv_maxclients 8 \
    +set sv_pure 0 \
    +set sv_allowDownload 1 \
    +map "$MAP" \
    "${@:2}"
