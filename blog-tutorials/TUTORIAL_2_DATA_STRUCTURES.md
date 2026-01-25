# Tutorial 2: Debugging Data Structures

This tutorial shows how to debug complex data structures using debugger-cli. We'll find a bug in a linked list implementation.

## The Bug

Our linked list has a size tracking bug - when removing the head node, it forgets to decrement the size counter:

```
BUG DETECTED!
  Actual elements: 3
  Reported size: 4
  Difference: 1
```

## Step 1: Start Debugging

```bash
$ debugger start example2_linked_list.py --stop-on-entry
Started debugging: /home/user/debugger-cli/blog-tutorials/example2_linked_list.py
Stopped at entry point. Use 'debugger continue' to run.
```

## Step 2: Set Strategic Breakpoints

Let's set a breakpoint in the `remove` method:

```bash
$ debugger break example2_linked_list.py:47
Breakpoint 1 set at example2_linked_list.py:47
```

We can also set a conditional breakpoint that only triggers when removing the head node:

```bash
$ debugger break example2_linked_list.py:54 --condition "previous is None"
Breakpoint 2 set at example2_linked_list.py:54
```

## Step 3: Continue to Breakpoint

```bash
$ debugger continue
Continuing execution...

$ debugger await --timeout 10
Stopped at breakpoint
  Location: example2_linked_list.py:47
```

## Step 4: Inspect State

```bash
$ debugger context
Thread 1 stopped at /home/user/debugger-cli/blog-tutorials/example2_linked_list.py:47
In function: remove

     44 |         # Bug: This check is incomplete - we find the node but don't
     45 |         # properly update self.head when removing it
     46 |         current = self.head
->   47 |         previous = None
     48 |
     49 |         while current:

Locals:
  current = Node(1) (Node)
  self = <__main__.LinkedList object at 0x7ed6f39e6a50> (LinkedList)
  value = 3 (int)
```

We can see we're removing value `3` (not the head). Let's continue to when we remove the head.

## Step 5: Continue to Bug Location

```bash
$ debugger continue
Continuing execution...

$ debugger await --timeout 10
Stopped at breakpoint
  Location: example2_linked_list.py:54
```

Now let's check the context:

```bash
$ debugger context
Thread 1 stopped at /home/user/debugger-cli/blog-tutorials/example2_linked_list.py:54
In function: remove

     49 |         while current:
     50 |             if current.value == value:
     51 |                 if previous is None:
     52 |                     # Bug: We update head but return before decrementing size
     53 |                     self.head = current.next
->   54 |                     return True  # Bug: size not decremented!
     55 |                 else:
     56 |                     previous.next = current.next
     57 |                     self.size -= 1
     58 |                     return True

Locals:
  current = Node(1) (Node)
  previous = None (NoneType)
  self = <__main__.LinkedList object at 0x7ed6f39e6a50> (LinkedList)
  value = 1 (int)
```

## Step 6: The Bug Revealed!

We can see it clearly:
- We're at line 54, about to `return True`
- `previous is None` means we're removing the **head** node
- Line 53 updates `self.head = current.next` ✓
- But line 54 returns **without** decrementing `self.size`!

Compare to the else branch (lines 56-58):
- Line 56: `previous.next = current.next` ✓
- Line 57: `self.size -= 1` ✓ <- This is missing in the head case!
- Line 58: `return True`

Let's verify:

```bash
$ debugger print "self.size"
self.size = 4 (int)

$ debugger print "value"
value = 1 (int)
```

After this return, size will still be 4, but only 3 elements remain!

## Step 7: List Breakpoints

```bash
$ debugger breakpoint list
Breakpoints:
  ✓ 1 example2_linked_list.py:47
  ✓ 2 example2_linked_list.py:54 (condition: previous is None)
```

## The Fix

Add `self.size -= 1` before returning when removing the head:

```python
if previous is None:
    self.head = current.next
    self.size -= 1  # Add this line!
    return True
```

## Key Takeaways

1. **Conditional breakpoints** are powerful for complex bugs - use `--condition` to break only when specific conditions are met

2. **Print expressions** help verify state - use `debugger print "self.size"` to check values

3. **Compare code paths** - the bug was in the head-removal path but not the middle-removal path

4. **Context is key** - seeing the source code with locals helps understand the full picture

## Breakpoint Commands Summary

| Command | Description |
|---------|-------------|
| `debugger break <loc>` | Set breakpoint at file:line |
| `debugger break <loc> --condition "<expr>"` | Conditional breakpoint |
| `debugger breakpoint list` | List all breakpoints |
| `debugger breakpoint remove <id>` | Remove a breakpoint |
| `debugger breakpoint disable <id>` | Temporarily disable |
| `debugger breakpoint enable <id>` | Re-enable breakpoint |
