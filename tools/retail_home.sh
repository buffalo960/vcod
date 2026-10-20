# Sourced by run_server.sh and run_probe.sh: picks the retail server's homepath.
#
# In: HOME_DEFAULT (the version's module homepath, e.g. private/server), GAMEDIR
# (main or uo), MODULE (path under the homepath), PORT_USED. Out: HOMEPATH,
# absolute.
#
# COD_LNXDED_HOME, when set, is used as is. Otherwise each port gets its own
# homepath, private/homes/<basename of HOME_DEFAULT>-<port>, holding links to
# everything in HOME_DEFAULT's game dir except its logs, its archived config
# and loose maps/, so two servers on two ports never write one games_mp.log or
# load each other's probe gametypes. The archived config is deleted on every
# start: a cvar an earlier run archived (sv_hostname, scr_friendlyfire,
# sv_wwwDownload) would otherwise reach the next run's serverinfo.
# `pwd -P` resolves a worktree's private/ link to the main checkout, whose path
# has no '+' for the engine's command line to split on.
if [ -n "${COD_LNXDED_HOME:-}" ]; then
    [ -f "$COD_LNXDED_HOME/$MODULE" ] || {
        echo "no game module at $COD_LNXDED_HOME/$MODULE; fix COD_LNXDED_HOME" >&2; exit 1; }
    HOMEPATH="$(cd "$COD_LNXDED_HOME" && pwd)"
else
    [ -f "$HOME_DEFAULT/$MODULE" ] || {
        echo "no game module at $HOME_DEFAULT/$MODULE; set COD_LNXDED_HOME" >&2; exit 1; }
    _src="$(cd "$HOME_DEFAULT" && pwd -P)"
    HOMEPATH="$(cd "$_src/.." && pwd -P)/homes/$(basename "$_src")-$PORT_USED"
    mkdir -p "$HOMEPATH/$GAMEDIR"
    for _f in "$_src/$GAMEDIR"/*; do
        case "$(basename "$_f")" in
            *.log | *.cfg | maps) continue ;;
        esac
        ln -sfn "$_f" "$HOMEPATH/$GAMEDIR/"
    done
    rm -f "$HOMEPATH/$GAMEDIR"/*config_mp_server.cfg
    unset _src _f
fi
case "$HOMEPATH" in
    *+*) echo "homepath $HOMEPATH contains '+', which the engine's command line splits on;" \
        "set COD_LNXDED_HOME to a path without one" >&2; exit 1 ;;
esac
echo "retail homepath: $HOMEPATH" >&2
