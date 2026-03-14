package verify.controlflow;

public class DoWhileLoop {
    public static int incrementUntilPositive(int value) {
        do {
            value = value + 1;
        } while (value < 1);

        return value;
    }
}
