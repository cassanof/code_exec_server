#!/bin/bash

V_ENGINE=${ENGINE:-docker}
$V_ENGINE build -t code-exec .
$V_ENGINE container rm -f code-exec 2>/dev/null
sleep 1
$V_ENGINE run --name code-exec --restart always -d --ulimit nofile=1000000:1000000 -p 8000:8000 code-exec
