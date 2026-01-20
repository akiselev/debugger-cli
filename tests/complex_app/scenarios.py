import time
import threading
import sys

def recursive_depth(n):
    if n <= 0:
        return "bottom"
    # BREAKPOINT_MARKER: recursion_step
    return recursive_depth(n - 1)

def create_large_data():
    # deeply nested dict
    data = {"level": 0}
    current = data
    for i in range(100):
        current["next"] = {"level": i + 1}
        current = current["next"]
    
    # large list
    large_list = list(range(10000))
    
    # circular ref
    a = {"name": "a"}
    b = {"name": "b"}
    a["ref"] = b
    b["ref"] = a
    
    # BREAKPOINT_MARKER: large_data_done
    return data, large_list, a

def thread_worker(name, distinct_sleep):
    print(f"Thread {name} starting")
    time.sleep(distinct_sleep) # Different sleep times to stagger events
    x = 100
    # BREAKPOINT_MARKER: thread_work
    print(f"Thread {name} working")
    return x * 2

def run_threads():
    threads = []
    t1 = threading.Thread(target=thread_worker, args=("T1", 0.5))
    t2 = threading.Thread(target=thread_worker, args=("T2", 1.0))
    
    t1.start()
    t2.start()
    
    # BREAKPOINT_MARKER: threads_started
    t1.join()
    t2.join()
    print("Threads finished")

class CustomException(Exception):
    pass

def exception_handler():
    try:
        raise CustomException("oops")
    except CustomException as e:
        # BREAKPOINT_MARKER: catch_exception
        print("Caught expected exception")
