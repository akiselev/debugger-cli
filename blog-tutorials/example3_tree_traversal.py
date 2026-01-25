#!/usr/bin/env python3
"""
Tutorial 3: Debugging Recursive Algorithms - Tree Traversal Bug

This program implements binary tree traversals but has a bug
in the level-order (BFS) traversal that causes it to reverse
levels unexpectedly.
"""

from collections import deque


class TreeNode:
    def __init__(self, value, left=None, right=None):
        self.value = value
        self.left = left
        self.right = right

    def __repr__(self):
        return f"TreeNode({self.value})"


def build_sample_tree():
    """Build a sample binary tree.

           1
          / \
         2   3
        / \   \
       4   5   6
    """
    return TreeNode(
        1,
        TreeNode(2, TreeNode(4), TreeNode(5)),
        TreeNode(3, None, TreeNode(6))
    )


def inorder_traversal(node, result=None):
    """Inorder traversal: left, root, right."""
    if result is None:
        result = []
    if node:
        inorder_traversal(node.left, result)
        result.append(node.value)
        inorder_traversal(node.right, result)
    return result


def preorder_traversal(node, result=None):
    """Preorder traversal: root, left, right."""
    if result is None:
        result = []
    if node:
        result.append(node.value)
        preorder_traversal(node.left, result)
        preorder_traversal(node.right, result)
    return result


def level_order_traversal(root):
    """Level-order (BFS) traversal.

    BUG: Using a list as a queue with pop(0) instead of popleft()
    is O(n) per operation. But that's not the real bug...

    The real bug: We're adding children in wrong order AND
    we use insert(0, x) instead of append(x) for some reason!
    """
    if not root:
        return []

    result = []
    queue = [root]

    while queue:
        node = queue.pop(0)  # Inefficient but works
        result.append(node.value)

        # Bug: Adding right child before left, AND inserting at front!
        if node.right:
            queue.insert(0, node.right)  # Bug: should be append!
        if node.left:
            queue.insert(0, node.left)   # Bug: should be append!

    return result


def level_order_correct(root):
    """Correct level-order traversal for comparison."""
    if not root:
        return []

    result = []
    queue = deque([root])

    while queue:
        node = queue.popleft()
        result.append(node.value)

        if node.left:
            queue.append(node.left)
        if node.right:
            queue.append(node.right)

    return result


def test_traversals():
    """Test all traversal methods."""
    print("Binary Tree Traversal Tests")
    print("=" * 50)
    print("""
Sample tree:
       1
      / \\
     2   3
    / \\   \\
   4   5   6
""")

    tree = build_sample_tree()

    print("Inorder (left, root, right):")
    inorder = inorder_traversal(tree)
    print(f"  Result: {inorder}")
    print(f"  Expected: [4, 2, 5, 1, 3, 6]")
    print(f"  {'PASS' if inorder == [4, 2, 5, 1, 3, 6] else 'FAIL'}")

    print("\nPreorder (root, left, right):")
    preorder = preorder_traversal(tree)
    print(f"  Result: {preorder}")
    print(f"  Expected: [1, 2, 4, 5, 3, 6]")
    print(f"  {'PASS' if preorder == [1, 2, 4, 5, 3, 6] else 'FAIL'}")

    print("\nLevel-order (BFS, level by level):")
    level = level_order_traversal(tree)
    expected = [1, 2, 3, 4, 5, 6]
    print(f"  Result: {level}")
    print(f"  Expected: {expected}")

    if level != expected:
        print(f"  FAIL - BUG DETECTED!")
        correct = level_order_correct(tree)
        print(f"  Correct implementation returns: {correct}")
    else:
        print(f"  PASS")


if __name__ == "__main__":
    test_traversals()
