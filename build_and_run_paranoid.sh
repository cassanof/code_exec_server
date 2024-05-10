#!/bin/bash

# check if gVisor is installed
if ! command -v runsc &> /dev/null
then
    echo "runsc could not be found"
    echo "Please install gVisor to run in paranoid mode"
    exit
fi

V_ENGINE=${ENGINE:-docker}
$V_ENGINE build -t code-exec .
$V_ENGINE container rm -f code-exec 2>/dev/null
sleep 1
$V_ENGINE run --name code-exec --restart always --runtime=runsc -d --ulimit nofile=1000000:1000000 -p 8000:8000 code-exec
