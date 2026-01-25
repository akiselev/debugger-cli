# Tutorial 3: Debugging Recursive Algorithms

This tutorial demonstrates debugging recursive code and tree traversals using debugger-cli. We'll track down a bug in a level-order (BFS) tree traversal.

## The Bug

Our level-order traversal should visit nodes level by level (1 → 2,3 → 4,5,6), but instead produces a depth-first-like result:

```
Expected: [1, 2, 3, 4, 5, 6]
Got:      [1, 2, 4, 5, 3, 6]
```

## The Tree Structure

```
       1
      / \
     2   3
    / \   \
   4   5   6
```

## Step 1: Start Debugging

```bash
$ debugger start example3_tree_traversal.py --stop-on-entry
Started debugging
Stopped at entry point. Use 'debugger continue' to run.
```

## Step 2: Set Breakpoints at Key Locations

Set a breakpoint where children are added to the queue:

```bash
$ debugger break example3_tree_traversal.py:81
Breakpoint 1 set at example3_tree_traversal.py:81
```

## Step 3: Continue to First Iteration

```bash
$ debugger continue
Continuing execution...

$ debugger await --timeout 10
Stopped at breakpoint
  Location: example3_tree_traversal.py:81
```

## Step 4: Examine the Queue State

```bash
$ debugger context
Thread 1 stopped at /home/user/debugger-cli/blog-tutorials/example3_tree_traversal.py:81
In function: level_order_traversal

     76 |     while queue:
     77 |         node = queue.pop(0)
     78 |         result.append(node.value)
     79 |
     80 |         # Bug: Adding right child before left, AND inserting at front!
->   81 |         if node.right:
     82 |             queue.insert(0, node.right)  # Bug: should be append!
     83 |         if node.left:
     84 |             queue.insert(0, node.left)   # Bug: should be append!

Locals:
  node = TreeNode(1) (TreeNode)
  queue = [] (list)
  result = [1] (list)
```

We just processed node 1. Queue is empty. Now we're about to add children.

## Step 5: Check What Children Will Be Added

```bash
$ debugger print "node.left"
node.left = TreeNode(2) (TreeNode)

$ debugger print "node.right"
node.right = TreeNode(3) (TreeNode)
```

Node 1 has left child 2 and right child 3.

## Step 6: Continue to See Queue After Adding Children

```bash
$ debugger continue
$ debugger await --timeout 10

$ debugger context
In function: level_order_traversal

Locals:
  node = TreeNode(2) (TreeNode)
  queue = [TreeNode(3)] (list)
  result = [1, 2] (list)
```

**The Bug Is Revealed!**

- We processed node **2** before node **3**!
- result = [1, 2] but it should be [1] after first iteration, then [1, 2, 3] after second
- The queue has [TreeNode(3)] meaning 3 is waiting, but we already processed 2

## Understanding the Bug

The buggy code uses `queue.insert(0, ...)` instead of `queue.append(...)`:

```python
# Buggy:
if node.right:
    queue.insert(0, node.right)  # Inserts at FRONT
if node.left:
    queue.insert(0, node.left)   # Inserts at FRONT

# After processing node 1:
# queue.insert(0, TreeNode(3)) → queue = [3]
# queue.insert(0, TreeNode(2)) → queue = [2, 3]
# pop(0) gets 2 first!
```

This creates a **stack** behavior (LIFO) instead of **queue** behavior (FIFO)!

## The Fix

Change `insert(0, ...)` to `append(...)`:

```python
# Fixed:
if node.left:
    queue.append(node.left)   # Add to END
if node.right:
    queue.append(node.right)  # Add to END
```

## Step 7: Verify the Fix

Stop the session and test the corrected implementation:

```bash
$ debugger stop
Debug session stopped
```

The corrected `level_order_correct` function produces `[1, 2, 3, 4, 5, 6]`.

## Debugging Techniques for Recursion

### 1. Track State Variables
Use `debugger print` to check:
- Current node/value being processed
- Queue/stack contents
- Accumulated results

### 2. Set Conditional Breakpoints
Break only at specific conditions:
```bash
$ debugger break file.py:50 --condition "len(queue) > 2"
```

### 3. Use Backtrace for Recursion Depth
```bash
$ debugger backtrace
#0 level_order_traversal at example3_tree_traversal.py:81
#1 test_traversals at example3_tree_traversal.py:115
#2 <module> at example3_tree_traversal.py:128
```

For recursive functions, the backtrace shows the recursion depth.

### 4. Watch Key Data Structures
Check queue/stack after each operation:
```bash
$ debugger print "queue"
queue = [TreeNode(3)] (list)

$ debugger print "[n.value for n in queue]"
[n.value for n in queue] = [3] (list)
```

## Key Insights

1. **Queue vs Stack**: `insert(0, x)` + `pop(0)` = Stack (LIFO), while `append(x)` + `pop(0)` = Queue (FIFO)

2. **Order Matters**: For BFS, children must be added in the correct order (left before right) AND using queue semantics

3. **Visual Debugging**: The debugger's ability to show current state makes it easy to spot when the algorithm deviates from expected behavior

## Command Summary

| Command | Use Case |
|---------|----------|
| `debugger print "expr"` | Evaluate complex expressions like list comprehensions |
| `debugger context` | See source code and all local variables |
| `debugger backtrace` | Understand call stack / recursion depth |
| `debugger continue` | Run to next breakpoint |
| `debugger next` | Step over (useful for skipping recursive calls) |
| `debugger step` | Step into (follow into recursive calls) |
