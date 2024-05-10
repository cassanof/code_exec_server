from typing import List, Optional, Tuple
import requests
import json
import threading


def exec_test(server, code, test, timeout=30, timeout_on_client=False, stdin="") -> Tuple[bool, str]:
    """
    Executes a test against a code snippet.
    Produces true if the test passes, false otherwise.
    Also returns the output of the code (sterr if it fails, stdout if it passes).

    You can set test to an empty string if you want to execute the code without any tests
    and just check if it runs without errors.

    timeout_on_client: If true, the client will timeout after timeout+2 seconds.
    """
    code_with_tests = code + "\n\n" + test
    data = json.dumps({"code": code_with_tests, "timeout": timeout, "stdin": stdin})
    try:
        r = requests.post(
            server + "/py_exec",
            data=data,
            timeout=(timeout + 2) if timeout_on_client else None
        )
        lines = r.text.split("\n")
        resp = lines[0]
        outs = "\n".join(lines[1:])
        assert resp == "0" or resp == "1"
        return resp == "0", outs
    except Exception as e:
        print(e)
        return False, "Failed to execute program"


def exec_test_multipl_e(server, code, test, lang, timeout=30, timeout_on_client=False) -> Tuple[bool, str]:
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
        outs = outs["stdout"] if resp == 0 else outs["stderr"]
        return resp == 0, outs
    except Exception as e:
        print(e)
        return False, "Failed to execute program"


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
        t = threading.Thread(target=exec_test_threaded, args=(i, code, test, stdin))
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
