#!/bin/bash

V_ENGINE=${ENGINE:-docker}

if [ $($V_ENGINE images | grep -c code-exec) -eq 0 ]; then
  $V_ENGINE pull elleven11/code-exec
  $V_ENGINE tag elleven11/code-exec code-exec
fi

$V_ENGINE container rm -f code-exec 2>/dev/null
sleep 1
$V_ENGINE run --name code-exec --restart always -d --ulimit nofile=1000000:1000000 -p 8000:8000 code-exec
