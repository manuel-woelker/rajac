package verify.controlflow;

public class TryCatchMultipleHandlers {
    public static int run() {
        try {
            throw new IllegalArgumentException();
        } catch (IllegalArgumentException first) {
            return 1;
        } catch (RuntimeException second) {
            return 2;
        }
    }
}
