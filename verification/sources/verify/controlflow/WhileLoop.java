package verify.controlflow;

public class WhileLoop {
    public static int sumTo(int limit) {
        int sum = 0;
        int current = 0;

        while (current <= limit) {
            sum = sum + current;
            current = current + 1;
        }

        return sum;
    }
}
