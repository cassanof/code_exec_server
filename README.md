# A Containerized Server To Execute Code Remotely

Runs a dockerized server that allows you to execute Python code via HTTP requests.
All the Python code executions will run concurrently; the server will not block any requests.
This means that the server will not wait for the execution of the Python code to finish before accepting new requests.

## Running the Server

To run the server, you need to have Docker installed on your machine (preferably with the [gVisor runtime](https://gvisor.dev/docs/) for security reasons).
Then, you can just use the `./build_and_run.sh` script to build the container
and run the server on your machine.

If you are feeling dangerous, you can also just run the server
directly with `./run.sh`. You'll need rust installed on your machine to compile the server.

### Calling the Server

We provide a simple Python library to interact with the server, which you can find in the `./code_exec_reqs.py` file.

You can also use whatever http client to interact with the server. There are three endpoints:

- `/py_exec`: executes Python code. Expects a json with field `code` containing the Python code to be executed.
- `/any_exec`: executes code in any language. Expects a json with fields `code` containing the code to be executed and `lang` containing the language of the program.
  See list of supported languages below.
  It's preferred to run python with the `/py_exec` endpoint, as it doesn't have the overhead of running MultiPL-E evaluators.
- `/py_coverage`: executes the Python code and returns the coverage of the code. Expects a json with field `code` containing the Python code to be executed.

Both `/py_exec` and `/any_exec` will return the exit code of the program, followed by the stdout and stderr of the program after the newline character.

## Other Languages

Yes! The server supports about 29 languages:

- clj
- cpp
- cs
- dfy
- dlang
- elixir
- fs
- go
- hs
- java
- javascript
- julia
- lean
- lua
- luau
- matlab
- ocaml
- php
- pl
- python
- racket
- r
- ruby
- rust
- scala
- sh
- swift
- ts
- v

This is thanks to the MultiPL-E evaluators, check them out [here](https://github.com/nuprl/MultiPL-E).
