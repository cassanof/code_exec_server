# A Containerized Server To Execute Python Code
Runs a dockerized server that allows you to execute Python code via HTTP requests.
All the Python code executions will run concurrently; the server will not block any requests.
This means that the server will not wait for the execution of the Python code to finish before accepting new requests.

We provide a simple Python library to interact with the server, which you can find in the `./code_exec_reqs.py` file.
