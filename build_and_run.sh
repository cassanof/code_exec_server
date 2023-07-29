#!/bin/bash

docker build -t python-code-exec .
docker container rm -f python-code-exec 2>/dev/null
sleep 1
docker run --name python-code-exec --restart always -d -p 8000:8000 python-code-exec
