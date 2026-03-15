package verify.algorithms;

public class FibonacciIterative {
    public static int fib(int n) {
        if (n <= 1) {
            return n;
        }

        int previous = 0;
        int current = 1;

        for (int index = 2; index <= n; index = index + 1) {
            int next = previous + current;
            previous = current;
            current = next;
        }

        return current;
    }
}
