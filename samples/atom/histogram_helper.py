# Helper to build 'correctness' histogram

from qdk import Result
from qdk.widgets import Histogram


def expect_zeros_histogram(output):
    results = []

    for shot in output:
        has_loss = False
        has_one = False

        for result in shot:
            if result == Result.Loss:
                has_loss = True
            elif result == Result.One:
                has_one = True

        if has_loss and has_one:
            results.append("Flip & Loss")
        elif has_loss:
            results.append("Loss")
        elif has_one:
            results.append("Flip")
        else:
            results.append("Correct")

    return Histogram(results)
