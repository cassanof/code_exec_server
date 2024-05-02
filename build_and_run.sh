#!/bin/bash

V_ENGINE=${ENGINE:-docker}
$V_ENGINE build -t python-code-exec .
$V_ENGINE container rm -f python-code-exec 2>/dev/null
sleep 1
$V_ENGINE run --name python-code-exec --restart always -d -p 8000:8000 python-code-exec
