#!/usr/bin/env python3
"""
Tutorial 4: Advanced Debugging - Server Request Simulation

This program simulates a web server handling multiple requests.
It demonstrates debugging with:
- Conditional breakpoints (break on specific request types)
- Hit count breakpoints (break after N requests)
- Output capture (viewing logs)
- Expression evaluation (complex state inspection)
"""

import time
import random


class Request:
    def __init__(self, method, path, user_id=None):
        self.method = method
        self.path = path
        self.user_id = user_id
        self.timestamp = time.time()

    def __repr__(self):
        return f"Request({self.method} {self.path})"


class Response:
    def __init__(self, status, body=""):
        self.status = status
        self.body = body

    def __repr__(self):
        return f"Response({self.status})"


class RequestHandler:
    def __init__(self):
        self.request_count = 0
        self.error_count = 0
        self.users = {"alice": "admin", "bob": "user", "charlie": "user"}

    def handle(self, request):
        """Handle an incoming request."""
        self.request_count += 1
        print(f"[{self.request_count}] Handling {request}")

        # Route the request
        if request.path.startswith("/api/"):
            return self.handle_api(request)
        elif request.path.startswith("/admin/"):
            return self.handle_admin(request)
        else:
            return self.handle_static(request)

    def handle_api(self, request):
        """Handle API requests."""
        if request.path == "/api/users":
            return Response(200, str(list(self.users.keys())))
        elif request.path.startswith("/api/user/"):
            user_id = request.path.split("/")[-1]
            if user_id in self.users:
                return Response(200, f"User: {user_id}, Role: {self.users[user_id]}")
            else:
                self.error_count += 1
                return Response(404, "User not found")
        else:
            self.error_count += 1
            return Response(404, "API endpoint not found")

    def handle_admin(self, request):
        """Handle admin requests - requires admin role."""
        # Bug: This check has a flaw!
        if request.user_id and self.users.get(request.user_id) == "admin":
            return Response(200, "Admin panel access granted")
        else:
            self.error_count += 1
            # Bug: We log the error but return wrong status!
            print(f"[ERROR] Unauthorized admin access attempt by {request.user_id}")
            return Response(401, "Unauthorized")

    def handle_static(self, request):
        """Handle static file requests."""
        return Response(200, f"Static content for {request.path}")


def generate_test_requests():
    """Generate a sequence of test requests."""
    requests = [
        Request("GET", "/index.html"),
        Request("GET", "/api/users"),
        Request("GET", "/api/user/alice"),
        Request("GET", "/api/user/eve"),  # Non-existent user
        Request("GET", "/admin/dashboard", user_id="bob"),  # Should fail - not admin
        Request("GET", "/admin/dashboard", user_id="alice"),  # Should succeed
        Request("POST", "/api/submit"),  # Unknown endpoint
        Request("GET", "/style.css"),
        Request("GET", "/api/user/charlie"),
        Request("GET", "/admin/settings", user_id=None),  # No user
    ]
    return requests


def run_simulation():
    """Run the server simulation."""
    print("Server Request Simulation")
    print("=" * 50)

    handler = RequestHandler()
    requests = generate_test_requests()

    results = []
    for req in requests:
        response = handler.handle(req)
        results.append((req, response))
        print(f"  → {response}")
        print()

    print("=" * 50)
    print(f"Total requests: {handler.request_count}")
    print(f"Total errors: {handler.error_count}")

    # Verify results
    error_responses = [r for req, r in results if r.status >= 400]
    print(f"Error responses: {len(error_responses)}")

    return handler, results


if __name__ == "__main__":
    run_simulation()
