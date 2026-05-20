#!/bin/sh
# SPEAR unified entrypoint / SPEAR 统一镜像启动入口

set -e

args="$*"

if [ "${SPEAR_CONFIG:-}" != "" ]; then
  case " $args " in
    *" --config "* ) ;;
    *" --config="* ) ;;
    *" -c "* ) ;;
    *" -c="* ) ;;
    * )
      set -- "$@" --config "$SPEAR_CONFIG"
      ;;
  esac
fi

if [ "${1:-}" = "" ]; then
  if [ "${SPEAR_DEFAULT_CMD:-}" != "" ]; then
    set -- "$SPEAR_DEFAULT_CMD"
  else
    set -- sms
  fi
fi

if [ "${1#-}" != "$1" ]; then
  if [ "${SPEAR_DEFAULT_CMD:-}" != "" ]; then
    set -- "$SPEAR_DEFAULT_CMD" "$@"
  else
    set -- sms "$@"
  fi
fi

exec "$@"
