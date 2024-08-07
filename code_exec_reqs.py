from typing import List, Optional, Tuple
import hashlib
import time
import requests
import json
import threading
import os

EXECUTOR_URL = os.getenv("EXECUTOR_URL", None)
EXECUTOR_AUTH = os.getenv("EXECUTOR_AUTH", None)


def exec_test(
        server: str,
        code: str,
        test: str,
        timeout: int = 30,
        timeout_on_client: bool = False,
        stdin: str = "",
        json_resp: bool = True,
        testhash_repo: Optional[str] = None,
) -> Tuple[bool, str]:
    """
    Executes a test against a code snippet.
    Produces true if the test passes, false otherwise.
    Also returns the output of the code (sterr if it fails, stdout if it passes).

    You can set test to an empty string if you want to execute the code without any tests
    and just check if it runs without errors.

    timeout_on_client: If true, the client will timeout after timeout*2 seconds.
    If false, the server will timeout after timeout seconds.

    json_resp: If true, the response will be in json format.
    If false, the response will be in plain text ("<status>\n<output>") format.

    testhash_repo: If provided, the server will use the testhash to cache the tests server-side.
    """
    if EXECUTOR_URL is not None:  # override the server
        server = EXECUTOR_URL
    assert isinstance(timeout, int), "Timeout needs to be an integer"
    if testhash_repo is not None:
        test_md5 = hashlib.md5(test.encode()).hexdigest()
        testhash = (testhash_repo, test_md5)
    else:
        code += "\n\n" + test
        testhash = None

    d = {"code": code, "timeout": timeout, "stdin": stdin,
         "json_resp": json_resp, "testhash": testhash}
    if EXECUTOR_AUTH:
        assert json_resp, "Auth only works with json responses"
        d = {"args": d}
    data = json.dumps(d)
    while True:  # loop for server downtime
        try:
            headers = {"Content-Type": "application/json"}
            if EXECUTOR_AUTH:
                headers["Authorization"] = EXECUTOR_AUTH
            r = requests.post(
                server + "/py_exec" if not EXECUTOR_AUTH else server,
                data=data,
                timeout=(
                    timeout * 2) if timeout_on_client or EXECUTOR_AUTH else None,
                headers=headers
            )
            if json_resp:
                j = r.json()
                if "detail" in j:
                    raise Exception(j["detail"])
                if EXECUTOR_AUTH:
                    assert j["status"] == "SUCCESS", f"Something went wrong: " + \
                        str(j)
                    j = json.loads(j["result"]["result"])

                resp = str(j["status"])
                outs = j["output"]
            else:
                lines = r.text.split("\n")
                resp = lines[0]
                outs = "\n".join(lines[1:])
            assert resp == "0" or resp == "1"
            return resp == "0", outs
        except Exception as e:
            # check if the server is alive
            if not check_executor_alive(server):
                # wait for the server to come back up
                print("Request rejected, waiting 3 seconds and then retrying...")
                time.sleep(3)
                continue
            else:
                # genuine server error
                return False, "Failed to execute program: " + str(e)


def exec_test_multipl_e(server, code, test, lang, timeout=30, timeout_on_client=False) -> Tuple[bool, str]:
    assert isinstance(timeout, int), "Timeout needs to be an integer"
    code_with_tests = code + "\n\n" + test
    data = json.dumps(
        {"code": code_with_tests, "lang": lang, "timeout": timeout})
    try:
        r = requests.post(
            server + "/any_exec",
            data=data,
            timeout=(timeout + 2) if timeout_on_client else None
        )
        lines = r.text.split("\n")
        resp = lines[0]
        outs = "\n".join(lines[1:])
        assert resp == "0" or resp == "1"
        if outs.strip() == "Timeout":
            return False, "Timeout"
        # parse json
        try:
            outs = json.loads(outs)
        except Exception as e:
            return False, "Failed to parse output: " + str(e)

        # get real status code
        resp = outs["exit_code"]
        outs = outs["stdout"] + outs["stderr"]
        return resp == 0, outs
    except Exception as e:
        print(e)
        return False, "Failed to execute program: " + str(e)


def exec_test_batched(server, codes, tests, lang=None, timeout=30, timeout_on_client=False, stdins=None) -> List[Tuple[bool, str]]:
    """
    Executes a batch of tests against a batch of code snippets using threading.

    Lang defaults to python if not provided.
    """
    threads = []
    results: List[Optional[Tuple[bool, str]]] = [None] * len(codes)

    if lang and lang != "python":
        assert stdins is None, "stdins are not supported for non-python languages for now"

        def exec_fn(code, test, _): return exec_test_multipl_e(
            server, code, test, lang, timeout, timeout_on_client)
    else:
        def exec_fn(code, test, stdin): return exec_test(
            server, code, test, timeout, timeout_on_client, stdin=stdin)

    def exec_test_threaded(i, code, test, stdin):
        results[i] = exec_fn(code, test, stdin)

    stdins = stdins or [None] * len(codes)

    for i, (code, test, stdin) in enumerate(zip(codes, tests, stdins)):
        t = threading.Thread(target=exec_test_threaded,
                             args=(i, code, test, stdin))
        threads.append(t)
        t.start()

    for t in threads:
        t.join(timeout=timeout*2)

    results_new = []
    for r in results:
        if r is None:
            results_new.append((False, "Failed to execute program"))
        else:
            results_new.append(r)

    return results_new


def run_coverage(server, code, tests, timeout=60, timeout_on_client=False) -> int:
    """
    Executes a code snippet and tests it with a set of tests,
    then returns the coverage percentage using coverage.py.
    """
    assert isinstance(timeout, int), "Timeout needs to be an integer"
    tests_str = "\n".join(tests)
    code_with_tests = code + "\n\n" + tests_str
    data = json.dumps({"code": code_with_tests, "timeout": timeout})
    try:
        r = requests.post(
            server + "/py_coverage",
            data=data,
            timeout=(timeout + 20) if timeout_on_client else None
        )
        return int(r.text)
    except Exception as e:
        print(e)
        return -3


def run_coverage_batched(server, codes, tests, timeout=60, timeout_on_client=False) -> List[int]:
    """
    Executes a batch of code snippets and tests them with a set of tests,
    then returns the coverage percentage using coverage.py.
    """
    threads = []
    results: List[Optional[int]] = [None] * len(codes)

    def run_coverage_threaded(i, code, test):
        results[i] = run_coverage(
            server, code, test, timeout, timeout_on_client)

    for i, (code, test) in enumerate(zip(codes, tests)):
        t = threading.Thread(target=run_coverage_threaded,
                             args=(i, code, test))
        threads.append(t)
        t.start()

    for t in threads:
        t.join()

    results_new = []
    for r in results:
        if r is None:
            results_new.append((False, "Failed to execute program"))
        else:
            results_new.append(r)

    return results_new


def check_executor_alive(executor):
    try:
        r = requests.get(executor + "/")
        return r.status_code == 200 or r.status_code == 404
    except Exception:
        return False
