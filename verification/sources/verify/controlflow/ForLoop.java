package verify.controlflow;

public class ForLoop {
    public static int sumRange(int limit) {
        int sum = 0;

        for (int current = 0; current <= limit; current = current + 1) {
            sum = sum + current;
        }

        return sum;
    }
}
