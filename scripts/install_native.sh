#!/bin/sh
# No additional installer runtime. All interfaces share the private worker.
set -eu
SOURCE_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PREFIX="${CARTRIDGE_STUDIO_INSTALL_PREFIX:-$HOME/.local}"
DATA_DIR="${XDG_DATA_HOME:-$PREFIX/share}"
APP_DIR="$DATA_DIR/cartridge-studio"
LEGACY_APP_DIR="$DATA_DIR/inl-cartridge-studio"
COMPONENTS=''
usage() {
    cat <<'EOF'
Cartridge Studio installer

Run ./install.sh to choose which interfaces to install:
  GUI   Native graphical application
  TUI   Interactive terminal application
  CLI   Commands for scripts and automation

For unattended installation, specify the complete selection:
  ./install.sh --components gui,tui
  ./install.sh --components tui
  ./install.sh --components cli
  ./install.sh --all

The shared cartridge engine is always included. Reinstalling changes the
active selection; earlier installations, backups and settings are retained.
CARTRIDGE_STUDIO_INSTALL_PREFIX sets the install prefix (default: ~/.local).
EOF
}
fail() { printf '%s\n' "$*" >&2; exit 1; }
while [ "$#" -gt 0 ]; do
    case "$1" in
        --help|-h) usage; exit 0 ;;
        --all) [ -z "$COMPONENTS" ] || fail 'Choose either --all or --components once.'; COMPONENTS=gui,tui,cli ;;
        --components)
            [ "$#" -gt 1 ] || fail '--components needs a selection, for example: --components gui,tui'
            [ -z "$COMPONENTS" ] || fail 'Choose either --all or --components once.'
            shift; COMPONENTS=$1
            [ -n "$COMPONENTS" ] || fail 'Select at least one interface: gui, tui or cli.' ;;
        *) fail "Unknown option: $1. Run ./install.sh --help for usage." ;;
    esac
    shift
done
if [ -z "$COMPONENTS" ]; then
    [ -t 0 ] || fail 'No interactive terminal. Rerun with --components gui,tui,cli (or --all) to choose what to install.'
    DEFAULT=gui,tui,cli
    if [ -f "$APP_DIR/components" ]; then DEFAULT=$(cat "$APP_DIR/components");
    elif [ -f "$LEGACY_APP_DIR/components" ]; then DEFAULT=$(cat "$LEGACY_APP_DIR/components"); fi
    printf '\nCartridge Studio — choose your interfaces\n\n'
    printf '  1  GUI   Graphical application\n  2  TUI   Interactive terminal\n  3  CLI   Commands and scripts\n\n'
    printf 'Choose names or numbers, separated by commas (for example: 1,2).\n'
    printf 'Shared cartridge engine: included automatically.\n'
    printf 'Install [%s]: ' "$DEFAULT"
    IFS= read -r COMPONENTS || fail 'Installation cancelled; nothing was changed.'
    COMPONENTS=${COMPONENTS:-$DEFAULT}
fi
RAW=$(printf '%s' "$COMPONENTS" | tr '[:upper:]' '[:lower:]' | tr -d '[:space:]')
case "$RAW" in ''|,*|*,|*,,*) fail 'Select at least one interface, separated by commas: gui,tui,cli.' ;; esac
GUI=0; TUI=0; CLI=0
REMAINING=$RAW
while [ -n "$REMAINING" ]; do
    TOKEN=${REMAINING%%,*}
    case "$TOKEN" in
        gui|1) GUI=1 ;; tui|2) TUI=1 ;; cli|3) CLI=1 ;;
        *) fail "Unknown interface: $TOKEN. Choose gui, tui or cli (or 1, 2, 3)." ;;
    esac
    case "$REMAINING" in *,*) REMAINING=${REMAINING#*,} ;; *) REMAINING='' ;; esac
done
COMPONENTS=''
PROGRAMS=''
if [ "$GUI" = 1 ]; then COMPONENTS=gui; PROGRAMS=cartridge-studio; fi
if [ "$TUI" = 1 ]; then COMPONENTS="${COMPONENTS:+$COMPONENTS,}tui"; PROGRAMS="$PROGRAMS cartridge-tui"; fi
if [ "$CLI" = 1 ]; then COMPONENTS="${COMPONENTS:+$COMPONENTS,}cli"; PROGRAMS="$PROGRAMS cartridge"; fi
for program in cartridge-worker $PROGRAMS; do
    [ -x "$SOURCE_DIR/bin/$program" ] || fail "This archive is missing $program. Extract a complete release archive and rerun the installer."
done
[ -f "$SOURCE_DIR/VERSION" ] || fail 'This archive has no VERSION file. Download and extract a complete release archive.'
VERSION=$(cat "$SOURCE_DIR/VERSION")
case "$VERSION" in ''|*[!0-9A-Za-z.+-]*) fail 'The archive VERSION is invalid. Extract a complete release archive.' ;; esac
managed_launcher() {
    [ -f "$1" ] && { grep -q '^# Managed by Cartridge Studio installer\.$' "$1" || grep -Fq "$APP_DIR/releases/" "$1"; }
}
for program in $PROGRAMS; do
    launcher="$PREFIX/bin/$program"
    if [ -e "$launcher" ] || [ -L "$launcher" ]; then
        managed_launcher "$launcher" || fail "$launcher belongs to another installation. Move it aside or choose another CARTRIDGE_STUDIO_INSTALL_PREFIX, then rerun."
    fi
done
mkdir -p "$APP_DIR/releases" "$PREFIX/bin"
STAGING_DIR=$(mktemp -d "$APP_DIR/releases/.installing.XXXXXX")
trap 'rm -rf -- "$STAGING_DIR"' EXIT
trap 'exit 1' HUP INT TERM
SLUG=$(printf '%s' "$COMPONENTS" | tr ',' '-')
DEST_DIR="$APP_DIR/releases/$VERSION-$SLUG-${STAGING_DIR##*.}"
mkdir -p "$STAGING_DIR/bin" "$STAGING_DIR/lib"
for name in VERSION README.md docs licenses 70-cartridge-studio.rules runtime-requirements.json; do
    cp -a "$SOURCE_DIR/$name" "$STAGING_DIR/"
done
# One binary/catalog on disk; the worker and CLI still execute as separate processes.
cp -p "$SOURCE_DIR/bin/cartridge-worker" "$STAGING_DIR/bin/cartridge-worker"
cp -p "$SOURCE_DIR/cartridge-worker" "$STAGING_DIR/cartridge-worker"
for program in $PROGRAMS; do
    if [ "$program" = cartridge ]; then ln "$STAGING_DIR/bin/cartridge-worker" "$STAGING_DIR/bin/cartridge";
    else cp -p "$SOURCE_DIR/bin/$program" "$STAGING_DIR/bin/$program"; fi
    cp -p "$SOURCE_DIR/$program" "$STAGING_DIR/$program"
done
if [ "$GUI" = 1 ]; then cp -a "$SOURCE_DIR/lib/." "$STAGING_DIR/lib/";
else cp -p "$SOURCE_DIR/lib/libgcc_s.so.1" "$STAGING_DIR/lib/"; fi
printf '%s\n' "$COMPONENTS" > "$STAGING_DIR/components"
mv "$STAGING_DIR" "$DEST_DIR"
trap - EXIT HUP INT TERM
shell_quote() { printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"; }
for program in cartridge-studio cartridge-tui cartridge cartridge-worker; do
    launcher="$PREFIX/bin/$program"
    case " $PROGRAMS " in
        *" $program "*)
            temporary="$launcher.new-$$"
            {
                printf '#!/bin/sh\n# Managed by Cartridge Studio installer.\n'
                if [ -n "${CARTRIDGE_STUDIO_DATA_DIR:-}" ]; then printf 'export CARTRIDGE_STUDIO_DATA_DIR='; shell_quote "$CARTRIDGE_STUDIO_DATA_DIR"; printf '\n'; fi
                printf 'exec '; shell_quote "$DEST_DIR/$program"; printf ' "$@"\n'
            } > "$temporary"
            chmod 755 "$temporary"; mv -f "$temporary" "$launcher" ;;
        *) if managed_launcher "$launcher"; then rm -- "$launcher"; fi ;;
    esac
done
mkdir -p "$DATA_DIR/applications"
for component in gui tui; do
    if [ "$component" = gui ]; then enabled=$GUI; suffix=''; program=cartridge-studio;
    else enabled=$TUI; suffix=.Terminal; program=cartridge-tui; fi
    name="io.github.borjaburgos.CartridgeStudio$suffix.desktop"
    destination="$DATA_DIR/applications/$name"
    if [ "$enabled" = 1 ]; then
        desktop_exec=$(printf '%s' "$PREFIX/bin/$program" | sed 's/\\/\\\\/g; s/"/\\"/g; s/`/\\`/g; s/\$/\\$/g; s/%/%%/g')
        while IFS= read -r line; do
            case "$line" in Exec=*) printf 'Exec="%s" %%f\n' "$desktop_exec" ;; *) printf '%s\n' "$line" ;; esac
        done < "$SOURCE_DIR/$name" > "$destination.new-$$"
        printf 'X-CartridgeStudio-Managed=true\n' >> "$destination.new-$$"
        mv -f "$destination.new-$$" "$destination"
    elif [ -f "$destination" ] && grep -q '^Icon=io.github.borjaburgos.CartridgeStudio$' "$destination"; then
        rm -- "$destination"
    fi
done
if [ "$GUI" = 1 ] || [ "$TUI" = 1 ]; then
    mkdir -p "$DATA_DIR/icons/hicolor/scalable/apps"
    cp "$SOURCE_DIR/io.github.borjaburgos.CartridgeStudio.svg" "$DATA_DIR/icons/hicolor/scalable/apps/"
fi
# Remove only launchers/menu entries created by the previous product installer.
# Leave previous releases and all libraries in place for recovery.
for old in inl-cartridge-studio inl-tui inl inl-worker; do
    launcher="$PREFIX/bin/$old"
    if [ -f "$launcher" ] && { grep -q '^# Managed by INL Cartridge Studio installer\.$' "$launcher" || grep -Fq "$LEGACY_APP_DIR/releases/" "$launcher"; }; then
        rm -- "$launcher"
    fi
done
for suffix in '' .Terminal; do
    old="$DATA_DIR/applications/io.github.borjaburgos.INLCartridgeStudio$suffix.desktop"
    if [ -f "$old" ] && grep -q '^X-INL-Managed=true$' "$old"; then rm -- "$old"; fi
done
printf '%s\n' "$COMPONENTS" > "$APP_DIR/components.new-$$"
mv -f "$APP_DIR/components.new-$$" "$APP_DIR/components"
printf '\nInstalled Cartridge Studio %s: %s\n' "$VERSION" "$COMPONENTS"
if [ "$GUI" = 1 ]; then printf 'GUI: application menu or cartridge-studio\n'; fi
if [ "$TUI" = 1 ]; then printf 'TUI: terminal application menu or cartridge-tui\n'; fi
if [ "$CLI" = 1 ]; then printf 'CLI: cartridge --help\n'; fi
printf 'Backups and settings are unchanged. Rerun this installer to change interfaces.\n'
