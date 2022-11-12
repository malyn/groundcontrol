#!/usr/bin/env sh
set -Eeuo >/dev/null

DAEMON_NAME=$1
PID_BASE_PATH=$2

# Wait for the daemon PID to appear.
while [ ! -f ${PID_BASE_PATH}/${DAEMON_NAME}.pid ]; do
    sleep 0;
done