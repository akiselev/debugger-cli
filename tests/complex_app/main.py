import sys
import time
import scenarios

def main():
    print("Starting Complex App")
    
    if len(sys.argv) > 1:
        mode = sys.argv[1]
    else:
        mode = "all"

    if mode in ["all", "recursion"]:
        print("Running recursion test...")
        scenarios.recursive_depth(10)
        
    if mode in ["all", "data"]:
        print("Running data test...")
        scenarios.create_large_data()
        
    if mode in ["all", "threads"]:
        print("Running threads test...")
        scenarios.run_threads()
        
    if mode in ["all", "exception"]:
        print("Running exception test...")
        scenarios.exception_handler()

    print("Complex App Finished")
    return 0

if __name__ == "__main__":
    sys.exit(main())
