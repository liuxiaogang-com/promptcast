#!/bin/sh
# 只替换应用包，不访问 Application Support 中的用户数据库。
DMG="$1"; APP="$2"; APP_PID="$3"; JOB="$4"
STAGE=""; MNT=""
exec >>"$JOB/install.log" 2>&1
fail() { printf '%s\n' "$1" >"$JOB/error.txt"; exit 1; }
finish() {
  status=$?
  trap - EXIT HUP INT TERM
  if [ "$status" -ne 0 ]; then
    # 仅在目标位置为空时恢复，绝不递归删除目标位置的文件。
    if [ -n "$STAGE" ] && [ -d "$STAGE/previous.app" ] && [ ! -e "$APP" ]; then
      /bin/mv "$STAGE/previous.app" "$APP" || true
    fi
    printf '\nInstaller: %s\nBackup: %s\n' "$DMG" "$STAGE" >>"$JOB/error.txt"
    /usr/bin/open -a TextEdit "$JOB/error.txt"
    if ! kill -0 "$APP_PID" 2>/dev/null && [ -d "$APP" ]; then /usr/bin/open "$APP"; fi
  fi
  if [ -n "$MNT" ]; then /usr/bin/hdiutil detach "$MNT" -quiet || true; fi
  # 失败现场和旧包都保留；不删除安装包，也不清理可能用于恢复的数据。
  exit "$status"
}
trap finish EXIT
trap 'fail "Update interrupted; original application or backup has been preserved."' HUP INT TERM
[ -f "$DMG" ] && [ -d "$APP/Contents" ] || fail "Invalid installer or application path."
STAGE=$(/usr/bin/mktemp -d "${APP}.update.XXXXXX") || fail "Cannot create staging directory beside the application."
MNT=$(/usr/bin/mktemp -d "$JOB/mount.XXXXXX") || fail "Cannot create mount directory."
/usr/bin/hdiutil attach "$DMG" -readonly -nobrowse -noautoopen -mountpoint "$MNT" || fail "Cannot mount installer."
set -- "$MNT"/*.app
[ "$#" -eq 1 ] && [ -d "$1/Contents/MacOS" ] || fail "Installer must contain exactly one application."
/usr/bin/ditto "$1" "$STAGE/new.app" || fail "Cannot stage new application; original is unchanged."
/usr/bin/codesign --verify --deep --strict "$STAGE/new.app" || fail "New application signature is invalid."
printf 'ready\n' >"$JOB/ready" || fail "Cannot notify application."
i=0
while kill -0 "$APP_PID" 2>/dev/null; do
  [ "$i" -lt 100 ] || fail "Application did not exit; update cancelled."
  sleep 0.3
  i=$((i+1))
done
replace_app "$APP" "$STAGE" || fail "Cannot replace application; original or backup has been preserved."
if ! /usr/bin/open "$APP"; then
  /bin/mv "$APP" "$STAGE/failed.app" || fail "Cannot move failed application aside; backup retained."
  /bin/mv "$STAGE/previous.app" "$APP" || fail "Automatic restore failed; backup retained."
  fail "New application could not launch; original restored."
fi
printf 'Installed. Previous application retained at %s\n' "$STAGE/previous.app"
