#!/usr/bin/env sh
set -Eeuo >/dev/null

DAEMON_NAME=$1
RESULT_PATH=$2
PID_BASE_PATH=$3

# Write our startup flag to the result file.
echo ${DAEMON_NAME}:started >> ${RESULT_PATH}

# Start a five second background sleep and capture the PID.
sleep 5 &
SLEEP_PID=$!

# Trap the SIGTERM shutdown signal (which logs a shutdown-requested
# message, kills the sleep process, and then lets the script gracefully
# exit).
do_shutdown() {
    echo ${DAEMON_NAME}:shutdown-requested >> ${RESULT_PATH}
    kill ${SLEEP_PID}
}
trap 'do_shutdown' TERM

# Trap the SIGINT signal (which kills the sleep process and immediately
# exits the script; no additional messages).
do_stop() {
    kill ${SLEEP_PID}
    exit 1
}
trap 'do_stop' INT

# Write out the PID of the *this* process, which the test code can use
# to determine that the daemon has started, and to stop the daemon.
echo $$ > ${PID_BASE_PATH}/${DAEMON_NAME}.pid

# Wait for sleep to exit/be stopped, and ignore its exit code (otherwise
# the test-daemon script will fail).
wait ${SLEEP_PID} || true

# Clean exit.
echo ${DAEMON_NAME}:stopped >> ${RESULT_PATH}
exit 0