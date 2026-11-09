# 同目录重命名完成替换；不删除旧包，不覆盖已有备份。
replace_app() {
  [ -d "$1" ] && [ -d "$2/new.app" ] && [ ! -e "$2/previous.app" ] || return 1
  /bin/mv "$1" "$2/previous.app" || return 1
  if /bin/mv "$2/new.app" "$1"; then
    return 0
  fi
  /bin/mv "$2/previous.app" "$1" || return 2
  return 1
}
