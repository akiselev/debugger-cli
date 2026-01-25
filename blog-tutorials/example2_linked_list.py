#!/usr/bin/env python3
"""
Tutorial 2: Data Structure Debugging - Linked List with Memory Leak

This program implements a simple linked list but has a bug:
when removing nodes, it doesn't properly handle the case where
we're removing the head node, leading to lost references.
"""

class Node:
    def __init__(self, value):
        self.value = value
        self.next = None

    def __repr__(self):
        return f"Node({self.value})"


class LinkedList:
    def __init__(self):
        self.head = None
        self.size = 0

    def append(self, value):
        """Add a new node to the end of the list."""
        new_node = Node(value)
        if self.head is None:
            self.head = new_node
        else:
            current = self.head
            while current.next:
                current = current.next
            current.next = new_node
        self.size += 1

    def remove(self, value):
        """Remove the first node with the given value.

        BUG: This doesn't handle removing the head node correctly!
        """
        if self.head is None:
            return False

        # Bug: This check is incomplete - we find the node but don't
        # properly update self.head when removing it
        current = self.head
        previous = None

        while current:
            if current.value == value:
                if previous is None:
                    # Bug: We update head but return before decrementing size
                    self.head = current.next
                    return True  # Bug: size not decremented!
                else:
                    previous.next = current.next
                    self.size -= 1
                    return True
            previous = current
            current = current.next

        return False

    def to_list(self):
        """Convert linked list to Python list for easy viewing."""
        result = []
        current = self.head
        while current:
            result.append(current.value)
            current = current.next
        return result

    def __len__(self):
        return self.size


def test_linked_list():
    """Test the linked list implementation."""
    print("Testing Linked List Implementation")
    print("=" * 40)

    # Create a list
    ll = LinkedList()
    for i in range(1, 6):
        ll.append(i)

    print(f"Initial list: {ll.to_list()}")
    print(f"Size: {len(ll)}")

    # Remove middle element (works correctly)
    print("\nRemoving 3 (middle element)...")
    ll.remove(3)
    print(f"After removal: {ll.to_list()}")
    print(f"Size: {len(ll)}")  # Should be 4

    # Remove head element (triggers bug)
    print("\nRemoving 1 (head element)...")
    ll.remove(1)
    print(f"After removal: {ll.to_list()}")
    print(f"Size: {len(ll)}")  # Bug: Still shows 4, should be 3!

    # Verify bug
    actual_count = len(ll.to_list())
    reported_size = len(ll)

    print("\n" + "=" * 40)
    if actual_count != reported_size:
        print(f"BUG DETECTED!")
        print(f"  Actual elements: {actual_count}")
        print(f"  Reported size: {reported_size}")
        print(f"  Difference: {reported_size - actual_count}")
    else:
        print("All tests passed!")


if __name__ == "__main__":
    test_linked_list()
