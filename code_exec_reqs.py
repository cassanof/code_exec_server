import requests
import base64


def exec_test(server, code, test):
    """
    Executes a test against a code snippet.
    Produces true if the test passes, false otherwise.
    """
    code_with_tests = code + "\n\n" + test
    encoded = base64.b64encode(bytes(code_with_tests, "utf-8"))
    timeout = 10  # 5 for test exec, 5 for safety
    try:
        r = requests.post(
            server + "/py_exec",
            data=encoded,
            timeout=timeout)
        assert r.text == "0" or r.text == "1"
        return r.text == "0"
    except Exception as e:
        print(e)
        return False


def run_coverage(server, code, tests):
    tests_str = "\n".join(tests)
    code_with_tests = code + "\n\n" + tests_str
    encoded = base64.b64encode(code_with_tests.encode("utf-8"))
    timeout = 80  # 60 for run, 10 for report, 10 for safety
    try:
        r = requests.post(
            server + "/py_coverage",
            data=encoded,
            timeout=timeout,
        )
        return int(r.text)
    except Exception as e:
        print(e)
        return -3
