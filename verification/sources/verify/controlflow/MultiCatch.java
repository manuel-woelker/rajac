package verify.controlflow;

public class MultiCatch {
    public static int run() {
        try {
            throw new IllegalArgumentException();
        } catch (IllegalArgumentException | IllegalStateException err) {
            return 1;
        }
    }
}
