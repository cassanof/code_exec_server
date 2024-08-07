"""
Testing program to send a bunch of reqs
"""
import time
import code_exec_reqs

CODE_PASS = """
assert True
"""


CODE_FAIL = """
assert False, "This should fail"
"""

print("###### Testing simple pass/fail cases ######")

pass_req = code_exec_reqs.exec_test("http://127.0.0.1:8000", CODE_PASS, "")
print(pass_req)

fail_req = code_exec_reqs.exec_test("http://127.0.0.1:8000", CODE_FAIL, "")
print(fail_req)

print("##### Testing large code payload ######")

CODE_PASS_LARGE = (CODE_PASS * 100_000) + "\nprint('done pass large')"
pass_req = code_exec_reqs.exec_test("http://127.0.0.1:8000", CODE_PASS_LARGE, "")
print(pass_req)

CODE_FAIL_LARGE = "\nprint('doing fail large')\n" + (CODE_FAIL * 100_000)
fail_req = code_exec_reqs.exec_test("http://127.0.0.1:8000", CODE_FAIL_LARGE, "")
print(fail_req)

print("##### Testing stdin capture ######")
CODE_STDIN = """
inp = input()
assert inp == "Hello, World!"
print("stdin works")
"""
stdin_req = code_exec_reqs.exec_test(
    "http://127.0.0.1:8000", CODE_STDIN, "", stdin="Hello, World!")
print(stdin_req)

print("##### Testing string escape cases ######")
CODE_ESCAPE = """
print("Hello,'' World!")
print('a\\nb\\nc\\nd\\nef')
print("abc'def")
"""
escape_req = code_exec_reqs.exec_test(
    "http://127.0.0.1:8000", CODE_ESCAPE, "")
print(escape_req)

print("##### Testing memory overflow case ######")
CODE_OVERFLOW = """
import resource # limit memory to 4GB
resource.setrlimit(resource.RLIMIT_AS, (4 * 1024**3, 4 * 1024**3))
a = []
while True:
    a.append([42]*100000)
"""
codeoverflow_req = code_exec_reqs.exec_test(
        "http://127.0.0.1:8000", CODE_OVERFLOW, "")
print(codeoverflow_req)

print("###### Testing child trying to kill parent process ######")
CODE_KILL = """
import os
print(os.kill(os.getppid(), 9))
"""
kill_req = code_exec_reqs.exec_test(
    "http://127.0.0.1:8000", CODE_KILL, "")
print(kill_req)

print("###### Testing multiple pass/fail cases with Python ######")

pass_req = code_exec_reqs.exec_test_multipl_e(
    "http://127.0.0.1:8000", CODE_PASS, "", "python"
)
print(pass_req)

fail_req = code_exec_reqs.exec_test_multipl_e(
    "http://127.0.0.1:8000", CODE_FAIL,  "", "python"
)
print(fail_req)

print("###### Testing batched pass/fail cases with Python ######")

# batched
codes = [
    CODE_PASS,
    CODE_FAIL,
    CODE_PASS,
    CODE_FAIL,
    CODE_PASS,
]
tests = ["" for _ in range(len(codes))]
batched_req = code_exec_reqs.exec_test_batched(
    "http://127.0.0.1:8000", codes, tests)
print(batched_req)
assert len(batched_req) == len(codes)


print("###### Testing multiple pass/fail cases with TypeScript. also capture stdout/stderr ######")

CODE_TS_PASS = """
console.log("Hello, World!");
"""

# needs to exit 1 to fail
CODE_TS_FAIL = """
console.error("This should fail");
process.exit(1);
"""

pass_req = code_exec_reqs.exec_test_multipl_e(
    "http://127.0.0.1:8000", CODE_TS_PASS, "", "ts"
)
print(pass_req)

fail_req = code_exec_reqs.exec_test_multipl_e(
    "http://127.0.0.1:8000", CODE_TS_FAIL, "", "ts"
)

print(fail_req)

print("###### Testing multiple pass/fail cases with JavaScript. also capture stdout/stderr ######")

pass_req = code_exec_reqs.exec_test_multipl_e(
    "http://127.0.0.1:8000", CODE_TS_PASS, "", "javascript"
)
print(pass_req)

fail_req = code_exec_reqs.exec_test_multipl_e(
    "http://127.0.0.1:8000", CODE_TS_FAIL, "", "javascript"
)

print(fail_req)

print("###### Testing batched pass/fail cases with TypeScript ######")

# batched
codes = [
    CODE_TS_PASS,
    CODE_TS_FAIL,
    CODE_TS_PASS,
    CODE_TS_FAIL,
    CODE_TS_PASS,
]

tests = ["" for _ in range(len(codes))]
batched_req = code_exec_reqs.exec_test_batched(
    "http://127.0.0.1:8000", codes, tests, "ts")
print(batched_req)


print("###### Testing timeout case with Python ######")
# timeout on python
CODE_TIMEOUT = """
while True:
    pass
"""
time_now = time.time()
timeout_req = code_exec_reqs.exec_test(
    "http://127.0.0.1:8000", CODE_TIMEOUT, "", timeout=5)
time_after = time.time()
print("Time elapsed (seconds):", time_after - time_now)
print(timeout_req)
